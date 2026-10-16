//! Tables transcribed into the workspace from a pinned reference checkout (CoolProp's, AGA8's), checked against it
//! number for number.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::repo::Repo;

    /// The numeric tokens of the block of `text` after `start` up to the first `end`, as written.
    fn numbers<'a>(text: &'a str, start: &str, end: &str) -> Vec<&'a str> {
        let from = text.find(start).map_or(text.len(), |i| i + start.len());
        let block = text.get(from..).unwrap_or_default();
        let block = block.get(..block.find(end).unwrap_or(block.len())).unwrap_or_default();
        block
            .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '-' | '+')))
            .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
            .collect()
    }

    /// PLAN.md M11.5: phasekit-cubic's superancillary tables are v8.0.0's `cubicsuperancillary.h` (Bell and Deiters
    /// 2021), every coefficient and piece bound as the header writes it, in order, for each of the six expansions.
    #[test]
    fn cubic_superancillary_is_coolprops() {
        let repo = Repo::locate();
        let header = repo.read("reference/CoolProp/include/CoolProp/superancillary/cubicsuperancillary.h").unwrap();
        let tables = repo.read("crates/phasekit-cubic/src/superancillary/tables.rs").unwrap();
        for name in ["SRK_p", "SRK_rhoL", "SRK_rhoV", "PR_p", "PR_rhoL", "PR_rhoV"] {
            let want = numbers(&header, &format!("const auto {name} ="), "};");
            let got = numbers(&tables, &format!("const {}: &[Piece] = &[", name.to_uppercase()), "];");
            assert!(want.len() > 100, "{name}: {} numbers in the header", want.len());
            assert_eq!(got, want, "{name}");
        }
    }

    /// A loop or guard of AGA8's `SetupGERG`: `for (int v = lo; v <= hi; ++v)` or `if (v != n)`.
    enum Scope {
        For { var: char, lo: String, hi: usize },
        Not { var: char, value: usize },
    }

    /// An index expression of `SetupGERG`: a number, a loop variable or `variable + number`.
    fn index(expr: &str, env: &HashMap<char, usize>) -> Option<usize> {
        let expr: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
        match expr.split_once('+') {
            Some((a, b)) => Some(index(a, env)? + index(b, env)?),
            None => expr.parse().ok().or_else(|| expr.chars().next().and_then(|v| env.get(&v).copied())),
        }
    }

    /// Every binding of the loop variables `scopes` give, guards applied.
    fn environments(scopes: &[Scope]) -> Vec<HashMap<char, usize>> {
        let mut envs = vec![HashMap::new()];
        for scope in scopes {
            envs = match scope {
                Scope::For { var, lo, hi } => envs
                    .into_iter()
                    .flat_map(|env| {
                        let start = index(lo, &env).unwrap_or(usize::MAX);
                        (start..=*hi).map(move |v| {
                            let mut env = env.clone();
                            env.insert(*var, v);
                            env
                        })
                    })
                    .collect(),
                Scope::Not { var, value } => envs.into_iter().filter(|env| env.get(var) != Some(value)).collect(),
            };
        }
        envs
    }

    /// AGA8's `SetupGERG` (C++) as it assigns its tables: every assignment of a number before its `return;`, under its
    /// loops and guards, the last one winning as in C; keyed by array name and indices. The setup's rescaling after the
    /// data assigns expressions, which are skipped.
    fn setup_gerg(source: &str) -> HashMap<(String, Vec<usize>), f64> {
        let from = source.find("void SetupGERG()").and_then(|i| source[i..].find('{').map(|j| i + j + 1)).unwrap();
        let body = &source[from..from + source[from..].find("return;").unwrap()];
        let body: String = body.lines().map(|line| line.split("//").next().unwrap_or("")).collect::<Vec<_>>().join(" ");
        let (mut tables, mut scopes, mut rest) = (HashMap::new(), Vec::new(), body.trim_start());
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('}') {
                scopes.pop().unwrap();
                rest = after;
            } else if rest.starts_with("for") || rest.starts_with("if") {
                let close = rest.find(')').unwrap();
                scopes.push(scope(&rest[..close]));
                rest = rest[close + 1..].trim_start().strip_prefix('{').unwrap();
            } else {
                let end = rest.find(';').unwrap();
                assign(&rest[..end], &scopes, &mut tables);
                rest = &rest[end + 1..];
            }
            rest = rest.trim_start();
        }
        tables
    }

    /// A bound of `SetupGERG`'s loops: AGA8's array sizes or a number.
    fn bound(s: &str) -> usize {
        match s.trim() {
            "MaxFlds" => 21,
            "MaxMdl" => 10,
            "MaxTrmM" => 12,
            "MaxTrmP" => 24,
            n => n.parse().unwrap(),
        }
    }

    /// The scope a `for (...` or `if (...` header opens, its closing parenthesis cut off.
    fn scope(text: &str) -> Scope {
        let header = &text[text.find('(').unwrap() + 1..];
        if text.starts_with("for") {
            let parts: Vec<&str> = header.split(';').collect();
            let (var, lo) = parts[0].trim().trim_start_matches("int").split_once('=').unwrap();
            let hi = parts[1].split("<=").nth(1).unwrap();
            Scope::For { var: var.trim().chars().next().unwrap(), lo: lo.to_string(), hi: bound(hi) }
        } else {
            let (var, value) = header.split_once("!=").unwrap();
            Scope::Not { var: var.trim().chars().next().unwrap(), value: bound(value) }
        }
    }

    /// One statement `name[i]...[k] = <number>` under `scopes`, into `tables` for every binding; any other is skipped.
    fn assign(statement: &str, scopes: &[Scope], tables: &mut HashMap<(String, Vec<usize>), f64>) {
        let (target, value) = statement.split_once('=').unwrap_or(("", ""));
        let (Ok(value), Some((name, indices))) = (value.trim().parse::<f64>(), target.trim().split_once('[')) else {
            return;
        };
        let indices: Vec<&str> = indices.trim_end_matches(']').split("][").collect();
        for env in environments(scopes) {
            let key = indices.iter().map(|i| index(i, &env).unwrap()).collect();
            tables.insert((name.to_string(), key), value);
        }
    }

    /// AGA8's arrays indexed by a pair of components, which `SetupGERG` fills for i < j (and the reducing parameters'
    /// diagonal, which it sets to 1).
    const BINARY: [&str; 6] = ["bvij", "gvij", "btij", "gtij", "fij", "mNumb"];

    /// PLAN.md M13.5 (user decision MX7): phasekit-mix's GERG-2008 tables are AGA8's (`AGA8CODE/C/GERG2008.cpp` at the
    /// pinned commit), every number as `SetupGERG` assigns it before rescaling. Every entry of the transcription is
    /// AGA8's (0 where AGA8 assigns nothing, as C's statics are), and AGA8 assigns nothing the transcription leaves out
    /// but the reducing parameters' diagonal and the lower triangles it fills from the upper ones.
    #[test]
    fn gerg_2008_is_aga8s() {
        let repo = Repo::locate();
        let source = repo.read_bytes("reference/AGA8/AGA8CODE/C/GERG2008.cpp").unwrap();
        let aga8 = setup_gerg(&String::from_utf8_lossy(&source));
        let ours = phasekit_mix::internal::gerg_2008_aga8();
        assert!(ours.len() > 2_000, "{} entries", ours.len());
        let mut differ = Vec::new();
        for (name, indices, value) in &ours {
            let want = aga8.get(&(name.to_string(), indices.clone())).copied().unwrap_or(0.0);
            if want.to_bits() != value.to_bits() {
                differ.push(format!("{name}{indices:?}: {value} against AGA8's {want}"));
            }
        }
        assert!(differ.is_empty(), "{differ:#?}");
        let covered: HashMap<(&str, &Vec<usize>), ()> = ours.iter().map(|(n, i, _)| ((*n, i), ())).collect();
        let names: Vec<&str> = ours.iter().map(|(n, _, _)| *n).collect();
        let left: Vec<String> = aga8
            .iter()
            .filter(|((name, idx), _)| names.contains(&name.as_str()) && !covered.contains_key(&(name.as_str(), idx)))
            .filter(|((name, idx), _)| !(BINARY.contains(&name.as_str()) && idx[0] >= idx[1]))
            .map(|((name, idx), v)| format!("{name}{idx:?} = {v}"))
            .collect();
        assert!(left.is_empty(), "AGA8 assigns what the transcription leaves out: {left:#?}");
    }
}
