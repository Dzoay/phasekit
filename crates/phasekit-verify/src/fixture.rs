//! The `<kind>/v1` fixture format (VERIFICATION.md §3.3): a `#`-header CSV with one header line per key, a status
//! column, Python-`repr` floats that parse to the same bits, and an FNV-1a `bits:` checksum over every float cell, which
//! the reader recomputes (the M1 "fixture round trip bit-exact" gate). A violation of any rule is an error, never a
//! skipped row. Committed fixtures are read with `include_str!`, so `Fixture<'static>` borrows from the binary.

use crate::tolerance::{Provenance, ToleranceClass};

/// The header keys in the only order they may appear (VERIFICATION.md §3.3).
const KEYS: [&str; 12] =
    ["fixture", "oracle", "generator", "config", "env", "fluid", "grid", "source", "columns", "units", "tol", "bits"];

/// Keys every fixture has.
const REQUIRED: [&str; 4] = ["fixture", "source", "columns", "tol"];

/// Keys a fixture from the CoolProp oracle has besides those.
const REQUIRED_FROM_ORACLE: [&str; 6] = ["oracle", "generator", "config", "env", "fluid", "bits"];

/// The error classes of a failed oracle call (VERIFICATION.md §3.1): `status` is `ok` or `err:<class>`.
const ERROR_CLASSES: [&str; 4] = ["notimpl", "solver", "domain", "other"];

/// What a column holds, from its `tol:` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnRole {
    /// An input of the case (`in`): a float, compared bitwise.
    Input,
    /// Text compared as text (`label`): status, region, phase, path.
    Label,
    /// An output compared under a tolerance class (VERIFICATION.md §5).
    Output(ToleranceClass),
}

/// One cell: a float (every column but labels) or text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cell<'a> {
    /// A float cell, parsed from its Python `repr`.
    Num(f64),
    /// A label cell.
    Text(&'a str),
}

/// One data row and the line it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Row<'a> {
    /// 1-based line in the file.
    pub line: usize,
    /// One cell per column.
    pub cells: Vec<Cell<'a>>,
}

/// A parsed fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture<'a> {
    name: &'a str,
    kind: &'a str,
    header: Vec<(&'a str, &'a str)>,
    columns: Vec<&'a str>,
    roles: Vec<ColumnRole>,
    rows: Vec<Row<'a>>,
}

/// A rule of the format that the text breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureError {
    /// The fixture's name (its path).
    pub fixture: String,
    /// 1-based line, 0 for the file as a whole.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.fixture, self.line, self.message)
    }
}

/// A value that disagrees with the fixture: everything map 10 U1 asks a failure to print.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureMismatch {
    /// The fixture's name.
    pub fixture: String,
    /// 1-based line of the row.
    pub line: usize,
    /// Column name.
    pub column: String,
    /// Tolerance class of the column (`exact` for inputs).
    pub class: &'static str,
    /// The fixture's `source:`.
    pub provenance: String,
    /// The value under test.
    pub got: f64,
    /// The fixture's value.
    pub want: f64,
    /// |got − want| (NaN when either is NaN).
    pub error: f64,
}

impl std::fmt::Display for FixtureMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: column {} (class {}, provenance {}): got {:?}, want {:?}, error {:e}",
            self.fixture, self.line, self.column, self.class, self.provenance, self.got, self.want, self.error
        )
    }
}

/// Why a check did not pass.
#[derive(Clone, Debug, PartialEq)]
pub enum CheckError {
    /// The value disagrees with the fixture.
    Mismatch(FixtureMismatch),
    /// No such row, no such column, or a label column.
    NoNumber {
        /// Row index asked for.
        row: usize,
        /// Column asked for.
        column: String,
    },
    /// The column's class has no bound yet: the class table lands at M1.5.
    NoBound {
        /// The class.
        class: ToleranceClass,
    },
}

impl<'a> Fixture<'a> {
    /// Parses `text`, read from `name`.
    pub fn parse(name: &'a str, text: &'a str) -> Result<Self, FixtureError> {
        let error = |line: usize, message: String| FixtureError { fixture: name.to_string(), line, message };
        if text.contains('\r') {
            return Err(error(0, "lines end in LF only".to_string()));
        }
        let mut lines = text.lines().enumerate().map(|(i, line)| (i + 1, line)).peekable();
        let mut header: Vec<(&'a str, &'a str)> = Vec::new();
        while let Some((n, line)) = lines.next_if(|(_, line)| line.starts_with('#')) {
            let (key, value) = line
                .strip_prefix("# ")
                .and_then(|rest| rest.split_once(": "))
                .ok_or_else(|| error(n, format!("a header line is `# key: value`, got `{line}`")))?;
            let position = KEYS
                .iter()
                .position(|known| *known == key)
                .ok_or_else(|| error(n, format!("unknown header key `{key}`")))?;
            if header.last().and_then(|(last, _)| KEYS.iter().position(|known| known == last)) >= Some(position) {
                return Err(error(n, format!("`{key}` repeats or is out of order; the order is {}", KEYS.join(", "))));
            }
            header.push((key, value));
        }
        let get = |key: &str| header.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
        let source = get("source").unwrap_or_default();
        let oracle_keys: &[&str] = if source == "coolprop" { &REQUIRED_FROM_ORACLE } else { &[] };
        if let Some(key) = REQUIRED.iter().chain(oracle_keys).find(|key| get(key).is_none()) {
            return Err(error(0, format!("header key `{key}` is required")));
        }
        if parse_source(source).is_none() {
            return Err(error(0, format!("unknown source `{source}`")));
        }
        let kind = get("fixture")
            .and_then(|f| f.strip_suffix("/v1"))
            .filter(|kind| !kind.is_empty() && !kind.contains('/'))
            .ok_or_else(|| error(0, "the format is `fixture: <kind>/v1`".to_string()))?;
        let columns: Vec<&'a str> = get("columns").unwrap_or_default().split(',').collect();
        let roles = get("tol")
            .unwrap_or_default()
            .split(',')
            .map(|tol| match tol {
                "in" => Some(ColumnRole::Input),
                "label" => Some(ColumnRole::Label),
                class => ToleranceClass::from_name(class).map(ColumnRole::Output),
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(0, "a `tol` entry is `in`, `label` or a class of VERIFICATION.md §5".to_string()))?;
        let units = get("units").map_or(columns.len(), |units| units.split(',').count());
        if columns.iter().any(|c| c.is_empty()) || roles.len() != columns.len() || units != columns.len() {
            return Err(error(0, "`columns`, `units` and `tol` name every column once".to_string()));
        }
        let status = columns.iter().position(|c| *c == "status");
        let mut rows = Vec::new();
        for (n, line) in lines {
            let cells: Vec<&'a str> = line.split(',').collect();
            if cells.len() != columns.len() {
                return Err(error(n, format!("{} cells, but {} columns", cells.len(), columns.len())));
            }
            if let Some(status) = status.and_then(|i| cells.get(i)).filter(|s| !valid_status(s)) {
                return Err(error(n, format!("status `{status}` is not `ok` or `err:<class>`")));
            }
            let cells = cells
                .iter()
                .zip(&roles)
                .map(|(cell, role)| match role {
                    ColumnRole::Label => Some(Cell::Text(cell)),
                    _ => parse_float(cell).map(Cell::Num),
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error(n, format!("a float cell is not a Python repr: `{line}`")))?;
            rows.push(Row { line: n, cells });
        }
        if rows.is_empty() {
            return Err(error(0, "no data rows".to_string()));
        }
        if let Some(bits) = get("bits") {
            let want = bits.strip_prefix("fnv1a64=").and_then(|hex| u64::from_str_radix(hex, 16).ok());
            let got = fnv1a64(&rows);
            if want != Some(got) {
                return Err(error(0, format!("bits: the file says `{bits}`, the cells give fnv1a64={got:016x}")));
            }
        }
        Ok(Fixture { name, kind, header, columns, roles, rows })
    }

    /// The kind, from `fixture: <kind>/v1`.
    pub fn kind(&self) -> &'a str {
        self.kind
    }

    /// A header value.
    pub fn header(&self, key: &str) -> Option<&'a str> {
        self.header.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    /// The column names.
    pub fn columns(&self) -> &[&'a str] {
        &self.columns
    }

    /// The column roles, from `tol:`.
    pub fn roles(&self) -> &[ColumnRole] {
        &self.roles
    }

    /// The data rows.
    pub fn rows(&self) -> &[Row<'a>] {
        &self.rows
    }

    /// The float in `row` (0-based) and `column`.
    pub fn value(&self, row: usize, column: &str) -> Option<f64> {
        let index = self.columns.iter().position(|c| *c == column)?;
        match self.rows.get(row)?.cells.get(index)? {
            Cell::Num(value) => Some(*value),
            Cell::Text(_) => None,
        }
    }

    /// Compares `got` with the fixture's value in `row` (0-based) and `column` under the column's class.
    pub fn check(&self, row: usize, column: &str, got: f64) -> Result<(), CheckError> {
        let no_number = || CheckError::NoNumber { row, column: column.to_string() };
        let want = self.value(row, column).ok_or_else(no_number)?;
        let class = match self.columns.iter().position(|c| *c == column).and_then(|i| self.roles.get(i)) {
            Some(ColumnRole::Output(class)) => *class,
            _ => ToleranceClass::Exact,
        };
        if class != ToleranceClass::Exact {
            return Err(CheckError::NoBound { class });
        }
        if canonical_bits(got) == canonical_bits(want) {
            return Ok(());
        }
        Err(CheckError::Mismatch(FixtureMismatch {
            fixture: self.name.to_string(),
            line: self.rows.get(row).map_or(0, |r| r.line),
            column: column.to_string(),
            class: class.name(),
            provenance: self.header("source").unwrap_or_default().to_string(),
            got,
            want,
            error: (got - want).abs(),
        }))
    }
}

impl Fixture<'static> {
    /// The provenance named by `source:` (VERIFICATION.md §3.3).
    pub fn provenance(&self) -> Provenance {
        let oracle_version = || {
            let oracle = self.header("oracle").unwrap_or_default();
            oracle.strip_prefix("CoolProp ").and_then(|rest| rest.split(' ').next()).unwrap_or(oracle)
        };
        match parse_source(self.header("source").unwrap_or_default()) {
            Some(Origin::Coolprop) => Provenance::Oracle { version: oracle_version() },
            Some(Origin::CoolpropSource { git }) => Provenance::Oracle { version: git },
            Some(Origin::Paper { citation, table }) => Provenance::Paper { citation, table },
            Some(Origin::Iapws { release }) => Provenance::Iapws { release },
            Some(Origin::MultiPrecision { source }) => Provenance::MultiPrecision { source },
            None => Provenance::SelfReferential, // parse() refuses an unknown source
        }
    }
}

/// The forms of `source:` (VERIFICATION.md §3.3).
enum Origin<'a> {
    Coolprop,
    CoolpropSource { git: &'a str },
    Paper { citation: &'a str, table: &'a str },
    Iapws { release: &'a str },
    MultiPrecision { source: &'a str },
}

fn parse_source(source: &str) -> Option<Origin<'_>> {
    let nonempty = |s: &str| !s.is_empty();
    if source == "coolprop" {
        return Some(Origin::Coolprop);
    }
    let (scheme, rest) = source.split_once(':')?;
    match scheme {
        "paper" => rest
            .split_once('/')
            .filter(|(c, t)| nonempty(c) && nonempty(t))
            .map(|(citation, table)| Origin::Paper { citation, table }),
        "iapws" => rest
            .split_once('/')
            .filter(|(r, t)| nonempty(r) && nonempty(t))
            .map(|(release, _)| Origin::Iapws { release }),
        "mp" => Some(rest).filter(|r| nonempty(r)).map(|source| Origin::MultiPrecision { source }),
        "coolprop-source" => rest
            .split_once('@')
            .filter(|(p, g)| nonempty(p) && nonempty(g))
            .map(|(_, git)| Origin::CoolpropSource { git }),
        _ => None,
    }
}

fn valid_status(status: &str) -> bool {
    status == "ok" || status.strip_prefix("err:").is_some_and(|class| ERROR_CLASSES.contains(&class))
}

/// A float as Python's `repr` writes it: `nan`, `inf`, `-inf`, or digits, sign, point and exponent only. Rust's parse
/// is correctly rounded, so the bits are the ones Python printed.
fn parse_float(token: &str) -> Option<f64> {
    match token {
        "nan" => Some(f64::NAN),
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        _ if token.bytes().all(|b| b.is_ascii_digit() || b"+-.e".contains(&b)) => token.parse().ok(),
        _ => None,
    }
}

/// The bits a float cell is hashed and compared as; every NaN is `0x7ff8000000000000`.
fn canonical_bits(value: f64) -> u64 {
    if value.is_nan() { 0x7ff8_0000_0000_0000 } else { value.to_bits() }
}

/// FNV-1a 64 over the little-endian bits of every float cell in row order (VERIFICATION.md §3.3).
fn fnv1a64(rows: &[Row]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for cell in rows.iter().flat_map(|row| &row.cells) {
        if let Cell::Num(value) = cell {
            for byte in canonical_bits(*value).to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORACLE_HEADER: &str = "\
# fixture: eos/v1
# oracle: CoolProp 8.0.0 git=ae81610e7d23efc57f9d051c8e70a4d66e87537f so_sha256=05d85591
# generator: gen.py sha256=00 python=3.12.14 libc=glibc-2.41
# config: {}
# env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C
# fluid: Water json_sha256=00
# source: coolprop
# columns: value,region,status
# tol: exact,label,label
";

    /// Values as Python's `repr` writes them; the checksum is the one Python computes with `struct.pack('<d')`.
    const VALUES: [(&str, u64); 11] = [
        ("5e-324", 0x0000_0000_0000_0001),
        ("2.2250738585072014e-308", 0x0010_0000_0000_0000),
        ("-0.0", 0x8000_0000_0000_0000),
        ("0.0", 0x0000_0000_0000_0000),
        ("1e-300", 0x01a5_6e1f_c2f8_f359),
        ("1.7976931348623157e+308", 0x7fef_ffff_ffff_ffff),
        ("nan", 0x7ff8_0000_0000_0000),
        ("inf", 0x7ff0_0000_0000_0000),
        ("-inf", 0xfff0_0000_0000_0000),
        ("0.1", 0x3fb9_9999_9999_999a),
        ("300.0", 0x4072_c000_0000_0000),
    ];
    const BITS: &str = "# bits: fnv1a64=2ebc28ebd94ee28b\n";

    fn fixture_text() -> String {
        let rows: String = VALUES.iter().map(|(v, _)| format!("{v},stable,ok\n")).collect();
        format!("{ORACLE_HEADER}{BITS}{rows}")
    }

    fn parse(text: String) -> Result<Fixture<'static>, FixtureError> {
        Fixture::parse("eos/Water.csv", String::leak(text))
    }

    /// Map 10 §8.3: every finite value round-trips bit-exactly through Python `repr` and Rust `parse`.
    #[test]
    fn fixture_round_trip_is_bit_exact() {
        let fixture = parse(fixture_text()).unwrap();
        assert_eq!((fixture.kind(), fixture.columns()), ("eos", &["value", "region", "status"][..]));
        assert_eq!(fixture.roles(), [ColumnRole::Output(ToleranceClass::Exact), ColumnRole::Label, ColumnRole::Label]);
        for (i, (printed, bits)) in VALUES.iter().enumerate() {
            let value = fixture.value(i, "value").unwrap();
            assert_eq!(value.to_bits(), *bits, "{printed}");
        }
        assert_eq!(fixture.rows()[0].line, 11);
        assert_eq!(fixture.rows()[0].cells[1], Cell::Text("stable"));
        // One flipped bit in one value breaks the checksum.
        let flipped = fixture_text().replace("\n0.1,", "\n0.10000000000000002,");
        assert!(parse(flipped).unwrap_err().message.contains("bits"));
        // The same values spelled in ways Python's repr never writes are refused, though Rust would parse them.
        for (repr, token) in
            [("nan", "NaN"), ("inf", "Infinity"), ("-inf", "-Infinity"), ("300.0", "3E2"), ("300.0", "")]
        {
            let text = fixture_text().replace(&format!("\n{repr},"), &format!("\n{token},"));
            assert!(parse(text).is_err(), "{token:?}");
        }
        assert_eq!(parse_float("1e+16").map(f64::to_bits), Some(1e16_f64.to_bits()));
        assert_eq!(parse_float("0x10"), None);
    }

    /// VERIFICATION.md §3.3: required, ordered, known, unique header keys; exact cell counts; statuses; LF only.
    #[test]
    fn header_fields_are_required() {
        let missing = fixture_text()
            .replace("# oracle: CoolProp 8.0.0 git=ae81610e7d23efc57f9d051c8e70a4d66e87537f so_sha256=05d85591\n", "");
        let error = parse(missing).unwrap_err();
        assert_eq!(error.to_string(), "eos/Water.csv:0: header key `oracle` is required");
        // A printed table needs no oracle keys, but always fixture, source, columns and tol.
        let paper = "# fixture: eos/v1\n# source: paper:lemmon2016/7\n# columns: p\n# tol: paper\n21.17909\n";
        assert!(parse(paper.to_string()).is_ok());
        assert!(parse(paper.replace("# tol: paper\n", "")).is_err());
        let broken = [
            fixture_text().replace("# config: {}\n", "# config: {}\n# config: {}\n"),
            fixture_text().replace("# env:", "# colour:"),
            fixture_text().replace(
                "# config: {}\n# env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C\n",
                "# env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C\n# config: {}\n",
            ),
            fixture_text().replace("# fixture: eos/v1", "# fixture: eos/v2"),
            fixture_text().replace("# fixture: eos/v1", "# fixture: /v1"),
            fixture_text().replace("# fixture: eos/v1", "# fixture: eos/x/v1"),
            fixture_text().replace("# source: coolprop", "# source: refprop"),
            fixture_text().replace("# tol: exact,label,label", "# tol: exact,label"),
            fixture_text().replace("# tol: exact,label,label", "# tol: exact,label,approx"),
            fixture_text().replace("300.0,stable,ok", "300.0,stable"),
            fixture_text().replace("300.0,stable,ok", "300.0,stable,err:bad"),
            fixture_text().replace("300.0,stable,ok\n", "300.0,stable,ok\r\n"),
            fixture_text().replace("300.0,stable,ok\n", "300.0,stable,ok\n\n"),
            fixture_text().replace("# columns:", "#columns:"),
        ];
        for (i, text) in broken.into_iter().enumerate() {
            assert!(parse(text).is_err(), "case {i} was accepted");
        }
        let failed = fixture_text().replace("300.0,stable,ok", "300.0,stable,err:solver");
        assert!(parse(failed).is_ok(), "err:<class> is a valid status");
    }

    /// Map 10 U1: a failure names the fixture, the row, the column, its class and the provenance.
    #[test]
    fn mismatch_reports_row_column_class_and_provenance() {
        let fixture = parse(fixture_text()).unwrap();
        assert_eq!(fixture.check(9, "value", 0.1), Ok(()));
        let Err(CheckError::Mismatch(m)) = fixture.check(9, "value", 0.10000000000000002) else { panic!() };
        assert_eq!((m.line, m.column.as_str(), m.class, m.provenance.as_str()), (20, "value", "exact", "coolprop"));
        let report = m.to_string();
        assert!(report.starts_with("eos/Water.csv:20: column value (class exact, provenance coolprop)"), "{report}");
        assert!(report.contains("got 0.10000000000000002, want 0.1, error 1.3877787807814457e-17"), "{report}");
        assert_eq!(m.error.to_bits(), (0.10000000000000002_f64 - 0.1).to_bits());
        // NaN equals NaN bitwise here: "no number from the oracle" is reproduced, not an error.
        assert_eq!(fixture.check(6, "value", f64::NAN), Ok(()));
        assert!(matches!(fixture.check(9, "region", 0.1), Err(CheckError::NoNumber { .. })));
        assert!(matches!(fixture.check(11, "value", 0.1), Err(CheckError::NoNumber { .. })));
        assert_eq!(fixture.provenance(), Provenance::Oracle { version: "8.0.0" });
        let paper = parse("# fixture: eos/v1\n# source: paper:lemmon2016/7\n# columns: p\n# tol: prop\n1.5\n".into());
        let paper = paper.unwrap();
        assert_eq!(paper.provenance(), Provenance::Paper { citation: "lemmon2016", table: "7" });
        assert_eq!(paper.check(0, "p", 1.5), Err(CheckError::NoBound { class: ToleranceClass::Prop }));
    }

    /// VERIFICATION.md §3.3: each `source:` form maps to its provenance; a form with an empty part is refused.
    #[test]
    fn every_source_form_has_its_provenance() {
        let fixture =
            |source: &str| parse(format!("# fixture: eos/v1\n# source: {source}\n# columns: p\n# tol: in\n1.0\n"));
        let provenance = |source: &str| fixture(source).map(|f| f.provenance());
        assert_eq!(provenance("iapws:R6-95(2018)/6"), Ok(Provenance::Iapws { release: "R6-95(2018)" }));
        assert_eq!(provenance("mp:coolprop-json"), Ok(Provenance::MultiPrecision { source: "coolprop-json" }));
        assert_eq!(
            provenance("coolprop-source:src/DataStructures.cpp@ae81610e"),
            Ok(Provenance::Oracle { version: "ae81610e" })
        );
        for bad in [
            "paper:/7",
            "paper:lemmon2016/",
            "iapws:/6",
            "iapws:R6-95/",
            "mp:",
            "coolprop-source:@ae81610e",
            "coolprop-source:src/x.cpp@",
            "refprop:10",
        ] {
            assert!(fixture(bad).is_err(), "{bad}");
        }
    }
}
