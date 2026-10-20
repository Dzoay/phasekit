//! What the oracle checks assert of a fixture's cells (PLAN.md M20.4a, user decision PL3): per fluid, kind and column,
//! the cells compared, those that disagreed, the cells each register entry exempts (by its DIV id) and the largest
//! error / bound of the cells that held; per fluid and kind, the oracle-gap rows (VERIFICATION.md §11.4). The five
//! oracle checks (`EosCheck`, `TermCheck`, `SaturationCheck`, `FlashCheck` and the props corpus's `PropsCheck`) keep
//! one each; the nightly sweep writes theirs beside the full set ([`Tallies::to_text`]), and
//! `cargo xtask nightly-report` reads them back ([`Tallies::read`]).

use std::collections::BTreeMap;

/// The first line of a tallies file.
const HEADER: &str = "# tallies/v1";

/// One column's cells of one fluid and kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Column {
    /// Cells compared with the oracle's, whether they held or not.
    pub asserted: usize,
    /// Of those, the ones that did not hold.
    pub failed: usize,
    /// The largest error / bound of the cells that held.
    pub headroom: f64,
    /// Cells not compared, by the id of the register entry that exempts them.
    pub exempt: BTreeMap<String, usize>,
}

/// One fluid's cells of one kind: its columns, and the rows the oracle failed where phasekit gives a value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    /// The columns by name.
    pub columns: BTreeMap<String, Column>,
    /// The oracle-gap rows (an `err:` status, a value here), counted (VERIFICATION.md §11.4).
    pub gaps: usize,
}

/// The tallies of one or more checks, by fluid and kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tallies {
    groups: BTreeMap<String, BTreeMap<String, Group>>,
}

impl Tallies {
    /// One cell of `column` of `fluid`'s `kind` compared: `held`, its error / bound, where it held, else `None`.
    pub fn cell(&mut self, at: (&str, &str), column: &str, held: Option<f64>) {
        self.column(at, column, |c| {
            c.asserted += 1;
            match held {
                Some(ratio) => c.headroom = c.headroom.max(ratio),
                None => c.failed += 1,
            }
        });
    }

    /// One cell of `column` of `fluid`'s `kind` that the register entry `id` exempts.
    pub fn exempt(&mut self, at: (&str, &str), column: &str, id: &str) {
        self.column(at, column, |c| with(&mut c.exempt, id, |n| *n += 1));
    }

    /// One oracle-gap row of `fluid`'s `kind`.
    pub fn gap(&mut self, at: (&str, &str)) {
        self.group(at, |g| g.gaps += 1);
    }

    /// Every group as (fluid, kind, group), by fluid, then kind.
    pub fn groups(&self) -> impl Iterator<Item = (&str, &str, &Group)> {
        self.groups
            .iter()
            .flat_map(|(fluid, kinds)| kinds.iter().map(move |(kind, g)| (fluid.as_str(), kind.as_str(), g)))
    }

    /// The tallies as text, a line per column, per exemption and per group with gaps, tab-separated:
    /// `cell <fluid> <kind> <column> <asserted> <failed> <headroom>`, `exempt <fluid> <kind> <column> <id> <cells>` and
    /// `gap <fluid> <kind> <rows>`.
    pub fn to_text(&self) -> String {
        let mut out = format!("{HEADER}\n");
        for (fluid, kind, group) in self.groups() {
            for (name, c) in &group.columns {
                out.push_str(&format!(
                    "cell\t{fluid}\t{kind}\t{name}\t{}\t{}\t{:?}\n",
                    c.asserted, c.failed, c.headroom
                ));
                for (id, n) in &c.exempt {
                    out.push_str(&format!("exempt\t{fluid}\t{kind}\t{name}\t{id}\t{n}\n"));
                }
            }
            if group.gaps > 0 {
                out.push_str(&format!("gap\t{fluid}\t{kind}\t{}\n", group.gaps));
            }
        }
        out
    }

    /// Adds the tallies in `text`, as [`Self::to_text`] writes them: counts add up and a column's headroom is the
    /// larger. The error names the line that is not a tally.
    pub fn read(&mut self, text: &str) -> Result<(), String> {
        let mut lines = text.lines().enumerate();
        if lines.next().map(|(_, l)| l) != Some(HEADER) {
            return Err(format!("line 1: not `{HEADER}`"));
        }
        for (i, line) in lines {
            self.line(line).ok_or_else(|| format!("line {}: not a tally: {line}", i + 1))?;
        }
        Ok(())
    }

    /// One line of [`Self::read`]'s text, or `None` where it is not one.
    fn line(&mut self, line: &str) -> Option<()> {
        let fields: Vec<&str> = line.split('\t').collect();
        let count = |i: usize| fields.get(i).and_then(|f| f.parse::<usize>().ok());
        match fields.as_slice() {
            ["cell", fluid, kind, column, _, _, headroom] => {
                let (asserted, failed, headroom) = (count(4)?, count(5)?, headroom.parse::<f64>().ok()?);
                self.column((*fluid, *kind), column, |c| {
                    (c.asserted, c.failed, c.headroom) =
                        (c.asserted + asserted, c.failed + failed, c.headroom.max(headroom));
                });
            }
            ["exempt", fluid, kind, column, id, _] => {
                let n = count(5)?;
                self.column((*fluid, *kind), column, |c| with(&mut c.exempt, id, |e| *e += n));
            }
            ["gap", fluid, kind, _] => {
                let n = count(3)?;
                self.group((*fluid, *kind), |g| g.gaps += n);
            }
            _ => return None,
        }
        Some(())
    }

    /// `f` on the group of (fluid, kind), a new one where there is none.
    fn group(&mut self, (fluid, kind): (&str, &str), f: impl FnOnce(&mut Group)) {
        with(&mut self.groups, fluid, |kinds| with(kinds, kind, f));
    }

    /// `f` on `column` of the group of `at`, a new one where there is none.
    fn column(&mut self, at: (&str, &str), column: &str, f: impl FnOnce(&mut Column)) {
        self.group(at, |g| with(&mut g.columns, column, f));
    }
}

/// `got`'s error against `want` over `bound`, where it is within the bound (NaN is not): what a cell that held
/// contributes to its column's headroom.
pub fn within(got: f64, want: f64, bound: f64) -> Option<f64> {
    let error = (got - want).abs();
    (error <= bound).then_some(error / bound)
}

/// `f` on the entry of `key` in `map`, a default one inserted first where it has none: the key is allocated only then,
/// so a check that counts millions of cells allocates once per column.
fn with<V: Default>(map: &mut BTreeMap<String, V>, key: &str, f: impl FnOnce(&mut V)) {
    match map.get_mut(key) {
        Some(v) => f(v),
        None => f(map.entry(key.to_string()).or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells, exemptions and gaps add up per fluid, kind and column, the headroom is the largest of the cells that
    /// held, and the text reads back to the same tallies, twice to twice the counts.
    #[test]
    fn cells_exemptions_and_gaps_add_up_and_read_back() {
        let mut tallies = Tallies::default();
        let (water, ses36) = (("Water", "eos"), ("SES36", "flash"));
        tallies.cell(water, "p", Some(0.25));
        tallies.cell(water, "p", Some(0.5));
        tallies.cell(water, "p", None);
        tallies.cell(water, "cp", Some(0.125));
        tallies.exempt(ses36, "p", "DIV-0019");
        tallies.exempt(ses36, "p", "DIV-0019");
        tallies.exempt(ses36, "p", "DIV-0020");
        tallies.gap(ses36);
        let groups: Vec<_> = tallies.groups().map(|(f, k, g)| (f, k, g.columns.len(), g.gaps)).collect();
        assert_eq!(groups, [("SES36", "flash", 1, 1), ("Water", "eos", 2, 0)]);
        let column = |t: &Tallies, (fluid, kind): (&str, &str), name: &str| {
            t.groups().find(|(f, k, _)| (*f, *k) == (fluid, kind)).unwrap().2.columns[name].clone()
        };
        let p = column(&tallies, water, "p");
        assert_eq!((p.asserted, p.failed, p.headroom, p.exempt.len()), (3, 1, 0.5, 0));
        let exempt = column(&tallies, ses36, "p").exempt;
        assert_eq!(exempt.into_iter().collect::<Vec<_>>(), [("DIV-0019".into(), 2), ("DIV-0020".into(), 1)]);
        let text = tallies.to_text();
        let lines = [
            HEADER,
            "cell\tSES36\tflash\tp\t0\t0\t0.0",
            "exempt\tSES36\tflash\tp\tDIV-0019\t2",
            "exempt\tSES36\tflash\tp\tDIV-0020\t1",
            "gap\tSES36\tflash\t1",
            "cell\tWater\teos\tcp\t1\t0\t0.125",
            "cell\tWater\teos\tp\t3\t1\t0.5",
        ];
        assert_eq!(text, format!("{}\n", lines.join("\n")), "no gap line for a group without one");
        let mut back = Tallies::default();
        assert_eq!(back.read(&text), Ok(()));
        assert_eq!(back, tallies);
        assert_eq!(back.read(&text), Ok(()));
        let p = column(&back, water, "p");
        assert_eq!((p.asserted, p.failed, p.headroom), (6, 2, 0.5));
        assert_eq!(column(&back, ses36, "p").exempt["DIV-0019"], 4);
        assert_eq!(back.groups().map(|(_, _, g)| g.gaps).sum::<usize>(), 2);
    }

    /// A file without the header, or with a line that is not a tally, is refused by its line.
    #[test]
    fn a_line_that_is_not_a_tally_is_named() {
        let mut tallies = Tallies::default();
        assert_eq!(tallies.read("cell\tWater\teos\tp\t1\t0\t0.5\n"), Err("line 1: not `# tallies/v1`".into()));
        let bad = ["cell\tWater\teos\tp\t1\t0", "cell\tWater\teos\tp\tone\t0\t0.5", "gap\tWater\teos\t-1", "row\tx"];
        for line in bad {
            assert_eq!(
                tallies.read(&format!("{HEADER}\ngap\tWater\teos\t1\n{line}\n")),
                Err(format!("line 3: not a tally: {line}"))
            );
        }
        let ok = format!("{HEADER}\nexempt\tWater\teos\tp\tDIV-0001\t7\ncell\tWater\teos\tp\t2\t0\tNaN\n");
        let mut fresh = Tallies::default();
        assert_eq!(fresh.read(&ok), Ok(()));
        let (_, _, group) = fresh.groups().next().unwrap();
        assert_eq!(
            (group.columns["p"].asserted, group.columns["p"].exempt["DIV-0001"], group.columns["p"].headroom),
            (2, 7, 0.0)
        );
    }

    /// A cell holds up to and including its bound, and its headroom is error / bound; NaN never holds.
    #[test]
    fn within_its_bound_is_inclusive() {
        assert_eq!(within(1.0, 1.5, 1.0), Some(0.5));
        assert_eq!(within(1.5, 1.0, 0.5), Some(1.0));
        assert_eq!(within(1.0, 2.5, 1.0), None);
        assert_eq!(within(f64::NAN, 1.0, 1.0), None);
    }
}
