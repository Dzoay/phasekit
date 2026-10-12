//! `gates complexity` (PLAN.md M7.9c; user decisions CX2, CX4): every function's cognitive complexity at most 15 and
//! its control-flow nesting at most 3, as Sonar's S3776 and S134 measure them, or exempted in
//! [`EXEMPTIONS`] with its reason and its ceilings. An exemption whose function no longer crosses a bar, or is gone,
//! fails: the list only shrinks.
//!
//! Cognitive complexity follows G. Ann Campbell, *Cognitive Complexity: a new way of measuring understandability*
//! (SonarSource, version 1.7, 2023), Appendix B, in Rust. `if`, `match` (a switch), `for`, `while` and `loop` (a
//! `while (true)`) add one plus their nesting level; `else if` and `else`, a let-else's `else`, a labelled `break` or
//! `continue`, each sequence of like `&&` or `||` operators (parentheses aside) and a direct recursive call add one.
//! Their bodies, closures and nested `fn`s raise the nesting level, and a nested `fn` counts in its outer function. A
//! ternary is Rust's `if`-`else`, `?` adds nothing, and a macro's tokens are not parsed; indirect recursion is not
//! detected (it needs the call graph). Nesting (S134) is the depth of `if`, `match`, `for`, `while` and `loop`, an
//! else-if chain flat and closures transparent. Every `fn` under `crates/` is measured but `generated.rs` and the ban
//! probes; length is clippy's `too_many_lines` (G2).

use std::collections::BTreeMap;

use syn::visit::{self, Visit};
use syn::{
    BinOp, Block, Expr, ExprBinary, ExprBreak, ExprCall, ExprClosure, ExprContinue, ExprForLoop, ExprIf, ExprLoop,
    ExprMatch, ExprMethodCall, ExprWhile, File, Ident, ImplItemFn, ItemFn, ItemImpl, ItemMod, ItemTrait, Local,
    TraitItemFn, Type,
};

use super::{Verdict, no_args};
use crate::repo::Repo;

/// S3776's bar: a cognitive complexity over 15 fails (user decision CX2).
const COGNITIVE: u32 = 15;

/// S134's bar: control flow nested deeper than 3 fails (user decision CX2).
const NESTING: u32 = 3;

/// The exemptions, one per line: `<file> <function> cog=<n> nest=<n> <reason>`, the function named as the gate names
/// it and its measures at most those ceilings; `#` starts a comment.
const EXEMPTIONS: &str = "ci/complexity-exemptions.txt";

/// A function's measures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Measure {
    /// Its cognitive complexity.
    cognitive: u32,
    /// Its deepest control-flow nesting.
    nesting: u32,
}

pub fn run(repo: &Repo, args: &[String]) -> Verdict {
    no_args(args)?;
    let files = repo.files("crates", ".rs", true).map_err(|e| vec![e])?;
    let measured: Vec<(String, String)> = files
        .into_iter()
        .filter(|(path, _)| !path.ends_with("/generated.rs") && !path.starts_with("crates/phasekit-xtask/probes/"))
        .collect();
    let exemptions = repo.read(EXEMPTIONS).map_err(|e| vec![e])?;
    check(&measured, &exemptions)
}

/// Every function of `file`, named `module::Type::name` within it, with its measures.
fn functions(file: &File) -> Vec<(String, Measure)> {
    let mut collector = Collector { scope: Vec::new(), found: Vec::new() };
    collector.visit_file(file);
    collector.found
}

/// Checks the (path, text) files against the bars and `exemptions` (the text of [`EXEMPTIONS`]).
fn check(files: &[(String, String)], exemptions: &str) -> Verdict {
    let mut errors = Vec::new();
    let measured = measure_all(files, &mut errors);
    let exempt = exemption_list(exemptions, &mut errors);
    errors.extend(measured.iter().filter_map(|(key, m)| judge(key, *m, exempt.get(key))));
    for key in exempt.keys().filter(|key| !measured.contains_key(*key)) {
        errors.push(format!("{EXEMPTIONS}: `{key}`: no such function"));
    }
    let summary = format!("{} functions in {} files, {} exempted", measured.len(), files.len(), exempt.len());
    if errors.is_empty() { Ok(summary) } else { Err(errors) }
}

/// Every function of the files keyed `<file> <function>`, with its measures; a file that does not parse is an error.
fn measure_all(files: &[(String, String)], errors: &mut Vec<String>) -> BTreeMap<String, Measure> {
    let mut measured = BTreeMap::new();
    for (path, text) in files {
        match syn::parse_file(text) {
            Ok(file) => measured.extend(functions(&file).into_iter().map(|(name, m)| (format!("{path} {name}"), m))),
            Err(e) => errors.push(format!("{path}: does not parse: {e}")),
        }
    }
    measured
}

/// The exemptions of `text` by key, with their ceilings; a malformed or repeated line is an error.
fn exemption_list(text: &str, errors: &mut Vec<String>) -> BTreeMap<String, Measure> {
    let mut exempt = BTreeMap::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#')) {
        match exemption(line) {
            Some((key, ceiling)) if exempt.insert(key.clone(), ceiling).is_some() => {
                errors.push(format!("{EXEMPTIONS}:{}: `{key}` is exempted twice", n + 1));
            }
            Some(_) => {}
            None => errors.push(format!("{EXEMPTIONS}:{}: not `<file> <function> cog=<n> nest=<n> <reason>`", n + 1)),
        }
    }
    exempt
}

/// The violation of one function against the bars and its exemption's `ceiling`, if any.
fn judge(key: &str, m: Measure, ceiling: Option<&Measure>) -> Option<String> {
    let shown = format!("{key}: cognitive {}, nest {}", m.cognitive, m.nesting);
    let over = m.cognitive > COGNITIVE || m.nesting > NESTING;
    match ceiling {
        Some(c) if m.cognitive > c.cognitive || m.nesting > c.nesting => {
            Some(format!("{shown}, over its exemption's cog={} nest={}", c.cognitive, c.nesting))
        }
        Some(_) if !over => Some(format!("{shown}, under the bars: drop its exemption")),
        None if over => Some(format!("{shown}, over the bars ({COGNITIVE}, {NESTING}) (PLAN.md M7.9c)")),
        _ => None,
    }
}

/// One exemption line: its key (`<file> <function>`) and its ceilings, or `None` if malformed or without a reason.
fn exemption(line: &str) -> Option<(String, Measure)> {
    let mut words = line.split_whitespace();
    let (file, function) = (words.next()?, words.next()?);
    let cognitive = words.next()?.strip_prefix("cog=")?.parse().ok()?;
    let nesting = words.next()?.strip_prefix("nest=")?.parse().ok()?;
    words.next()?;
    Some((format!("{file} {function}"), Measure { cognitive, nesting }))
}

/// The functions of one file: modules and the types of `impl` and `trait` blocks name its scope.
struct Collector {
    /// The enclosing modules and types.
    scope: Vec<String>,
    /// Each function measured, in source order.
    found: Vec<(String, Measure)>,
}

impl Collector {
    /// Measures one function: its body is everything inside it, nested `fn`s and closures included.
    fn measure(&mut self, name: &Ident, block: &Block, method: bool) {
        let mut walk = Walk { name, method, level: 0, depth: 0, measure: Measure::default(), recursive: false };
        walk.visit_block(block);
        walk.measure.cognitive += u32::from(walk.recursive);
        let name = name.to_string();
        let path = self.scope.iter().map(String::as_str).chain([name.as_str()]).collect::<Vec<_>>();
        self.found.push((path.join("::"), walk.measure));
    }

    /// `f` within the scope `name`.
    fn within(&mut self, name: String, f: impl FnOnce(&mut Self)) {
        self.scope.push(name);
        f(self);
        self.scope.pop();
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        self.within(m.ident.to_string(), |c| visit::visit_item_mod(c, m));
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        let name = match &*i.self_ty {
            Type::Path(p) => p.path.segments.last().map_or_else(|| "_".into(), |s| s.ident.to_string()),
            _ => "_".into(),
        };
        self.within(name, |c| visit::visit_item_impl(c, i));
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        self.within(t.ident.to_string(), |c| visit::visit_item_trait(c, t));
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        self.measure(&f.sig.ident, &f.block, false);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        self.measure(&f.sig.ident, &f.block, true);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast TraitItemFn) {
        if let Some(block) = &f.default {
            self.measure(&f.sig.ident, block, true);
        }
    }
}

/// The walk of one function's body (Appendix B in Rust; the module's documentation).
struct Walk<'a> {
    /// The function's name, for a direct recursive call.
    name: &'a Ident,
    /// Whether it is a method, which recurses through `self` or `Self` only.
    method: bool,
    /// The nesting level of Appendix B2: control structures' bodies, closures and nested `fn`s.
    level: u32,
    /// The control-flow depth of S134: `if`, `match` and the loops only.
    depth: u32,
    /// The measures so far.
    measure: Measure,
    /// Whether it calls itself.
    recursive: bool,
}

impl Walk<'_> {
    /// A structural increment and its nesting increment (Appendix B1, B3).
    fn structure(&mut self) {
        self.measure.cognitive += 1 + self.level;
    }

    /// `f` one nesting level deeper (Appendix B2).
    fn nested(&mut self, f: impl FnOnce(&mut Self)) {
        self.level += 1;
        f(self);
        self.level -= 1;
    }

    /// `f` one control-flow level deeper (S134).
    fn deeper(&mut self, f: impl FnOnce(&mut Self)) {
        self.depth += 1;
        self.measure.nesting = self.measure.nesting.max(self.depth);
        f(self);
        self.depth -= 1;
    }

    /// An `if`'s condition and branches, then its `else`: a hybrid increment, and an `else if` continues the chain
    /// at the same depth.
    fn chain(&mut self, e: &ExprIf) {
        self.visit_expr(&e.cond);
        self.nested(|w| w.visit_block(&e.then_branch));
        if let Some((_, other)) = &e.else_branch {
            self.measure.cognitive += 1;
            match &**other {
                Expr::If(next) => self.chain(next),
                other => self.nested(|w| w.visit_expr(other)),
            }
        }
    }
}

/// `&&` as `Some(true)`, `||` as `Some(false)`, any other operator as `None`.
fn logical(op: &BinOp) -> Option<bool> {
    match op {
        BinOp::And(_) => Some(true),
        BinOp::Or(_) => Some(false),
        _ => None,
    }
}

/// The operators of a run of `&&` and `||` in source order (through parentheses), and its other operands.
fn sequence<'a>(e: &'a Expr, ops: &mut Vec<bool>, operands: &mut Vec<&'a Expr>) {
    match e {
        Expr::Paren(p) => sequence(&p.expr, ops, operands),
        Expr::Binary(b) => match logical(&b.op) {
            Some(and) => {
                sequence(&b.left, ops, operands);
                ops.push(and);
                sequence(&b.right, ops, operands);
            }
            None => operands.push(e),
        },
        _ => operands.push(e),
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_expr_if(&mut self, e: &'ast ExprIf) {
        self.structure();
        self.deeper(|w| w.chain(e));
    }

    fn visit_expr_match(&mut self, e: &'ast ExprMatch) {
        self.structure();
        self.deeper(|w| {
            w.visit_expr(&e.expr);
            w.nested(|w| e.arms.iter().for_each(|arm| w.visit_arm(arm)));
        });
    }

    fn visit_expr_for_loop(&mut self, e: &'ast ExprForLoop) {
        self.structure();
        self.deeper(|w| {
            w.visit_expr(&e.expr);
            w.nested(|w| w.visit_block(&e.body));
        });
    }

    fn visit_expr_while(&mut self, e: &'ast ExprWhile) {
        self.structure();
        self.deeper(|w| {
            w.visit_expr(&e.cond);
            w.nested(|w| w.visit_block(&e.body));
        });
    }

    fn visit_expr_loop(&mut self, e: &'ast ExprLoop) {
        self.structure();
        self.deeper(|w| w.nested(|w| w.visit_block(&e.body)));
    }

    fn visit_local(&mut self, l: &'ast Local) {
        if let Some(init) = &l.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.measure.cognitive += 1;
                self.nested(|w| w.visit_expr(diverge));
            }
        }
    }

    fn visit_expr_closure(&mut self, e: &'ast ExprClosure) {
        self.nested(|w| w.visit_expr(&e.body));
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        self.nested(|w| w.visit_block(&f.block));
    }

    fn visit_expr_break(&mut self, e: &'ast ExprBreak) {
        self.measure.cognitive += u32::from(e.label.is_some());
        visit::visit_expr_break(self, e);
    }

    fn visit_expr_continue(&mut self, e: &'ast ExprContinue) {
        self.measure.cognitive += u32::from(e.label.is_some());
    }

    fn visit_expr_binary(&mut self, e: &'ast ExprBinary) {
        if logical(&e.op).is_none() {
            return visit::visit_expr_binary(self, e);
        }
        let (mut ops, mut operands) = (Vec::new(), Vec::new());
        sequence(&e.left, &mut ops, &mut operands);
        ops.push(logical(&e.op) == Some(true));
        sequence(&e.right, &mut ops, &mut operands);
        let changes = ops.windows(2).filter(|pair| pair.first() != pair.last()).count();
        self.measure.cognitive += 1 + u32::try_from(changes).unwrap_or(u32::MAX);
        operands.into_iter().for_each(|operand| self.visit_expr(operand));
    }

    fn visit_expr_call(&mut self, e: &'ast ExprCall) {
        if let Expr::Path(p) = &*e.func {
            let segments: Vec<&Ident> = p.path.segments.iter().map(|s| &s.ident).collect();
            self.recursive |= match segments.as_slice() {
                [name] => !self.method && *name == self.name,
                [this, name] => self.method && *this == "Self" && *name == self.name,
                _ => false,
            };
        }
        visit::visit_expr_call(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast ExprMethodCall) {
        let on_self = matches!(&*e.receiver, Expr::Path(p) if p.path.is_ident("self"));
        self.recursive |= self.method && on_self && e.method == *self.name;
        visit::visit_expr_method_call(self, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The white paper's examples in Rust (`cognitive_complexity_follows_the_white_paper`).
    const WHITE_PAPER: &str = r##"
fn sum_of_primes(max: u32) -> u32 {
    let mut total = 0;
    'out: for i in 1..=max {
        for j in 2..i {
            if i % j == 0 {
                continue 'out;
            }
        }
        total += i;
    }
    total
}

fn get_words(number: u32) -> &'static str {
    match number {
        1 => "one",
        2 => "a couple",
        3 => "a few",
        _ => "lots",
    }
}

fn sequences(a: bool, b: bool, c: bool, d: bool, e: bool, f: bool) {
    if a && b && c || d || e && f {}
}

fn negated(a: bool, b: bool, c: bool) {
    if a && !(b && c) {}
}

fn my_method(condition1: bool, condition2: bool) {
    if condition1 {
        for _ in 0..10 {
            while condition2 {}
        }
    }
}

fn my_method2(condition1: bool) {
    let r = || {
        if condition1 {}
    };
    r();
}

fn overridden_symbol_from(class_type: &ClassType, name: &str) -> Option<Symbol> {
    if class_type.is_unknown() {
        return Some(unknown_method_symbol());
    }
    let mut unknown_found = false;
    let symbols = class_type.symbol().members().lookup(name);
    for override_symbol in symbols {
        if override_symbol.is_kind(MTH) && !override_symbol.is_static() {
            let method = override_symbol;
            if can_override(&method) {
                let overriding = check_overriding_parameters(&method, class_type);
                if overriding.is_none() {
                    if !unknown_found {
                        unknown_found = true;
                    }
                } else if overriding == Some(true) {
                    return Some(method);
                }
            }
        }
    }
    if unknown_found {
        return Some(unknown_method_symbol());
    }
    None
}

fn to_regexp(ant_pattern: &str, directory_separator: &str) -> String {
    let escaped = format!("\\{directory_separator}");
    let mut sb = String::from("^");
    let chars: Vec<char> = ant_pattern.chars().collect();
    let mut i = if ant_pattern.starts_with('/') || ant_pattern.starts_with('\\') { 1 } else { 0 };
    while i < chars.len() {
        let ch = chars[i];
        if SPECIAL_CHARS.contains(ch) {
            sb.push('\\');
            sb.push(ch);
        } else if ch == '*' {
            if i + 1 < chars.len() && chars[i + 1] == '*' {
                if i + 2 < chars.len() && is_slash(chars[i + 2]) {
                    sb.push_str("(?:.*");
                    sb.push_str(&escaped);
                    sb.push_str("|)");
                    i += 2;
                } else {
                    sb.push_str(".*");
                    i += 1;
                }
            } else {
                sb.push_str("[^");
                sb.push_str(&escaped);
                sb.push_str("]*?");
            }
        } else if ch == '?' {
            sb.push_str("[^");
            sb.push_str(&escaped);
            sb.push(']');
        } else if is_slash(ch) {
            sb.push_str(&escaped);
        } else {
            sb.push(ch);
        }
        i += 1;
    }
    sb.push('$');
    sb
}
"##;

    /// Rust's own structures (`else_if_chains_and_closures_nest_as_specified`).
    const RUST: &str = r##"
fn chain(x: u8) -> u8 {
    if x == 0 { 0 } else if x == 1 { 1 } else if x == 2 { 2 } else { 3 }
}

fn deep(v: &[u8]) {
    for a in v {
        match a {
            0 => {
                while ready() {
                    if *a > 1 {}
                }
            }
            _ => {}
        }
    }
}

fn closures(v: &[u8]) -> usize {
    v.iter().filter(|x| if **x > 1 { true } else { false }).count()
}

fn looped() -> u8 {
    'outer: loop {
        loop {
            if ready() {
                break 'outer;
            }
        }
    }
    0
}

fn guarded(v: &[Option<u8>]) -> Result<u8, ()> {
    for x in v {
        let Some(y) = x else { continue };
        let z = check(*y)?;
        assert!(z > 0 && z < 9);
    }
    Ok(0)
}

fn fact(n: u64) -> u64 {
    if n == 0 { 1 } else { n * fact(n - 1) }
}

fn outer() -> u8 {
    fn inner(x: u8) -> u8 {
        if x > 0 { 1 } else { 0 }
    }
    inner(1)
}

fn and3(a: bool, b: bool, c: bool) -> bool {
    a && b && c
}

fn grouped(a: bool, b: bool, c: bool) -> bool {
    a && (b && c)
}

mod m {
    struct S;

    impl S {
        fn walk(&self, n: u8) -> u8 {
            if n > 0 { self.walk(n - 1) } else { Self::walk(self, 0) }
        }

        fn pt(&self) -> u8 {
            pt(1)
        }

        fn via_self(&self, n: u8) -> u8 {
            Self::via_self(self, n)
        }

        fn via_method(&self, n: u8) -> u8 {
            self.via_method(n)
        }

        fn not_self(&self, other: &S, n: u8) -> u8 {
            Other::not_self(n) + Self::pt(self) + other.not_self(n) + self.via_method(n)
        }
    }

    trait T {
        fn default(&self, x: bool) -> u8 {
            if x { 1 } else { 0 }
        }
    }
}
"##;

    /// The measures of every function in `source`.
    fn measures(source: &str) -> Vec<(String, Measure)> {
        functions(&syn::parse_file(source).unwrap())
    }

    /// Campbell (version 1.7) in Rust, each function's total as the paper annotates it: sumOfPrimes 7 and getWords 1
    /// ("An illustration of the problem"); the boolean sequences 4 and 3 ("Increment for breaks in the linear flow");
    /// myMethod without its `catch` 6 and myMethod2's lambda 2 ("Increment for nested flow-break structures");
    /// Appendix C's overriddenSymbolFrom 19, and toRegexp 20 and one more in Rust, where its ternary is an `if` with an
    /// `else`.
    #[test]
    fn cognitive_complexity_follows_the_white_paper() {
        let cognitive: Vec<(String, u32)> =
            measures(WHITE_PAPER).into_iter().map(|(name, measure)| (name, measure.cognitive)).collect();
        let want = [
            ("sum_of_primes", 7),
            ("get_words", 1),
            ("sequences", 4),
            ("negated", 3),
            ("my_method", 6),
            ("my_method2", 2),
            ("overridden_symbol_from", 19),
            ("to_regexp", 21),
        ];
        assert_eq!(cognitive, want.map(|(name, n)| (name.to_string(), n)));
    }

    /// Nesting (S134) and Rust's own structures: an else-if chain is as deep as its `if`; a `match` arm, a loop's body
    /// and a branch nest one deeper; a closure raises cognitive nesting (Appendix B2) but not control-flow depth, as
    /// does a nested `fn`, which counts in its outer function; `loop` is a `while (true)`; a let-else's `else` adds a
    /// hybrid one; a labelled `break` adds one; a run of like operators is one sequence, through parentheses; a direct
    /// recursive call adds one, a method's through `Self::` or `self.` (not another type's or value's function of its
    /// name, nor a free function of its name); `?` and a macro's tokens add nothing. A
    /// trait's default method is named by its trait.
    #[test]
    fn else_if_chains_and_closures_nest_as_specified() {
        let m = |cognitive, nesting| Measure { cognitive, nesting };
        let want = [
            ("chain", m(4, 1)),
            ("deep", m(10, 4)),
            ("closures", m(3, 1)),
            ("looped", m(7, 3)),
            ("guarded", m(2, 1)),
            ("fact", m(3, 1)),
            ("outer", m(3, 1)),
            ("and3", m(1, 0)),
            ("grouped", m(1, 0)),
            ("m::S::walk", m(3, 1)),
            ("m::S::pt", m(0, 0)),
            ("m::S::via_self", m(1, 0)),
            ("m::S::via_method", m(1, 0)),
            ("m::S::not_self", m(0, 0)),
            ("m::T::default", m(2, 1)),
        ];
        assert_eq!(measures(RUST), want.map(|(name, measure)| (name.to_string(), measure)));
    }

    /// The gate: a function over a bar fails unless exempted, one at a bar (cognitive 15, nesting 3) passes; an
    /// exemption needs its ceilings and a reason, holds only while its function stays at or under both ceilings and
    /// over a bar, and names a function that exists.
    #[test]
    fn exemptions_only_shrink() {
        let deep = "fn deep() { for _ in 0..1 { for _ in 0..1 { for _ in 0..1 { for _ in 0..1 {} } } } }\n";
        let files = [("crates/a/src/lib.rs".to_string(), format!("{deep}fn flat() {{}}\n"))];
        let exempt = |line: &str| check(&files, &format!("# exemptions\n{line}\n"));
        assert!(check(&files, "").is_err_and(|e| e.iter().any(|v| v.contains("deep") && v.contains("nest 4"))));
        assert!(exempt("crates/a/src/lib.rs deep cog=10 nest=4 M7.9d: flattened there").is_ok());
        let grew = exempt("crates/a/src/lib.rs deep cog=9 nest=4 M7.9d: flattened there");
        assert!(grew.is_err_and(|e| e.iter().any(|v| v.contains("over its exemption"))), "a ceiling is a ceiling");
        let shallow = exempt("crates/a/src/lib.rs deep cog=10 nest=3 M7.9d: flattened there");
        assert!(shallow.is_err_and(|e| e.iter().any(|v| v.contains("over its exemption"))), "so is a nesting ceiling");
        let stale = exempt("crates/a/src/lib.rs flat cog=0 nest=0 why");
        assert!(stale.is_err_and(|e| e.iter().any(|v| v.contains("flat") && v.contains("under the bars"))));
        let gone = exempt("crates/a/src/lib.rs gone cog=20 nest=5 why");
        assert!(gone.is_err_and(|e| e.iter().any(|v| v.contains("gone") && v.contains("no such function"))));
        for bad in ["crates/a/src/lib.rs deep cog=10 nest=4", "crates/a/src/lib.rs deep nest=4 cog=10 why", "deep"] {
            assert!(exempt(bad).is_err(), "{bad}");
        }
        let ifs = |n: usize| format!("fn ifs{n}(x: bool) {{ {} }}\n", "if x {} ".repeat(n));
        let at_the_bars = format!("{}fn three(a: bool) {{ if a {{ if a {{ if a {{}} }} }} }}\n", ifs(15));
        assert!(check(&[("crates/a/src/lib.rs".to_string(), at_the_bars)], "").is_ok(), "15 and 3 are at the bars");
        assert!(check(&[("crates/a/src/lib.rs".to_string(), ifs(16))], "").is_err(), "16 is over");
        let unparsable = [("crates/a/src/lib.rs".to_string(), "fn (".to_string())];
        assert!(check(&unparsable, "").is_err());
    }
}
