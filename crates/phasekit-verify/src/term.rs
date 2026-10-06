//! Checking `term` fixtures (VERIFICATION.md §3.5) against a compiled residual part, shared by the corpus tests and the
//! nightly sweep: each row is evaluated at the oracle's (T, ρ), so τ and δ are bitwise the oracle's, and every
//! `A_ij` is compared under class `Term` with the scale [`majorant::eos`], floored ([`majorant::floored`]).

use phasekit_core::internal::{EosRecord, FluidRecord};
use phasekit_core::{Error, HelmholtzModel, Order, PureFluid, math};

use crate::tolerance::TERM_FLOOR;
use crate::{CheckError, Fixture, majorant};

/// The 15 output columns of a `term` fixture, each with the (i, j) of the `A_ij` it holds unscaled.
pub const COLUMNS: [(&str, usize, usize); 15] = [
    ("alphar", 0, 0),
    ("dalphar_dtau", 1, 0),
    ("dalphar_ddelta", 0, 1),
    ("d2alphar_dtau2", 2, 0),
    ("d2alphar_ddelta_dtau", 1, 1),
    ("d2alphar_ddelta2", 0, 2),
    ("d3alphar_dtau3", 3, 0),
    ("d3alphar_ddelta_dtau2", 2, 1),
    ("d3alphar_ddelta2_dtau", 1, 2),
    ("d3alphar_ddelta3", 0, 3),
    ("d4alphar_dtau4", 4, 0),
    ("d4alphar_ddelta_dtau3", 3, 1),
    ("d4alphar_ddelta2_dtau2", 2, 2),
    ("d4alphar_ddelta3_dtau", 1, 3),
    ("d4alphar_ddelta4", 0, 4),
];

/// `e`'s constants and every residual list, without the ideal part.
pub fn residual_part(e: &EosRecord) -> EosRecord {
    let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
    eos.power.clone_from(&e.power);
    eos.lemmon2005.clone_from(&e.lemmon2005);
    eos.double_exponential.clone_from(&e.double_exponential);
    eos.gaussian.clone_from(&e.gaussian);
    eos.gao_b.clone_from(&e.gao_b);
    eos.non_analytic.clone_from(&e.non_analytic);
    eos
}

/// `eos` compiled under `record`'s name, molar mass, source and limits. `eos` carries no ideal part where the fluid's
/// ideal kinds have not landed yet: `residual` never reads it.
pub fn residual_model(record: &FluidRecord, eos: &EosRecord) -> Result<PureFluid, Error> {
    FluidRecord::new(&record.name, record.molar_mass, record.source.clone(), eos.clone(), record.limits).compile()
}

/// The running result of a `term` check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TermCheck {
    /// Entries compared.
    pub checked: usize,
    /// One line per entry outside `Term` (or row that could not be read), as map 10 U1 asks.
    pub failures: Vec<String>,
    /// The largest error / bound of the entries that passed.
    pub headroom: f64,
}

impl TermCheck {
    /// Checks `rows` (0-based) of `fixture` against `model`, whose residual part is `eos`.
    pub fn rows(&mut self, fixture: &Fixture<'_>, rows: &[usize], model: &dyn HelmholtzModel, eos: &EosRecord) {
        for &row in rows {
            let number = |column: &str| fixture.value(row, column);
            let (Some(t), Some(rho), Some(tau), Some(delta)) =
                (number("T"), number("rhomolar"), number("tau"), number("delta"))
            else {
                self.failures.push(format!("row {row}: T, rhomolar, tau or delta is missing"));
                continue;
            };
            // The model's τ and δ are the oracle's, bit for bit, or the comparison is not like for like.
            for (column, own) in [("tau", eos.t_reducing / t), ("delta", rho / eos.rho_reducing)] {
                if let Err(e) = fixture.check(row, column, own) {
                    self.failures.push(format!("row {row}: {column} differs from the oracle's: {e:?}"));
                }
            }
            let got = model.residual(t, rho, Order::Four);
            let alpha_scale = majorant::eos(eos, tau, delta, 0, 0);
            for (column, i, j) in COLUMNS {
                // The oracle's unscaled derivative is A_ij / (τ^i δ^j), and so is the scale, floored on A_ij itself
                // (an entry that is subnormal in scaled form keeps only a few digits there).
                let factor = math::powi(tau, i as i32) * math::powi(delta, j as i32);
                let scaled = majorant::floored(majorant::eos(eos, tau, delta, i, j), alpha_scale);
                let scale = scaled.max(TERM_FLOOR) / factor;
                let a = got.get(i, j).unwrap_or(f64::NAN);
                self.checked += 1;
                match fixture.check_scaled(row, column, a / factor, scale) {
                    Ok(ratio) => self.headroom = self.headroom.max(ratio),
                    Err(CheckError::Mismatch(m)) => self.failures.push(m.to_string()),
                    Err(e) => self.failures.push(format!("row {row}, {column}: {e:?}")),
                }
            }
        }
    }

    /// `None` when every entry passed, else a report of the first `shown` failures.
    pub fn report(&self, shown: usize) -> Option<String> {
        let lines = self.failures.iter().take(shown).cloned().collect::<Vec<_>>().join("\n");
        let (n, checked, headroom) = (self.failures.len(), self.checked, self.headroom);
        (n > 0).then(|| format!("{n} of {checked} entries outside Term (headroom {headroom:.3}):\n{lines}"))
    }
}

/// The `Term` scale of the α⁰ entries of one fluid (VERIFICATION.md §5): every ideal term's own contribution in
/// absolute value, each evaluated alone at ρ = ρ_r (where its ln δ is 0), plus |ln δ| for α⁰ itself and 1 for the exact
/// δ-entries `A_0j` (1, −1, 2).
#[derive(Debug)]
pub struct IdealScale {
    parts: Vec<PureFluid>,
    rho_r: f64,
}

impl IdealScale {
    /// One compiled part per ideal term of `record`.
    pub fn new(record: &FluidRecord) -> Result<IdealScale, Error> {
        let e = &record.eos;
        let mut parts = Vec::new();
        for term in &e.ideal {
            let mut eos = EosRecord::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max);
            eos.ideal = vec![*term];
            parts.push(residual_model(record, &eos)?);
        }
        Ok(IdealScale { parts, rho_r: e.rho_reducing })
    }

    /// The scale of α⁰ entry (i, j), `i + j ≤ 3`, at (T, ρ).
    pub fn get(&self, t: f64, rho: f64, i: usize, j: usize) -> f64 {
        if j > 0 {
            return if i == 0 { 1.0 } else { 0.0 };
        }
        let terms: f64 =
            self.parts.iter().map(|p| p.eos().ideal(t, self.rho_r, Order::Three).get(i, 0).unwrap_or(0.0).abs()).sum();
        if i == 0 { terms + math::ln(rho / self.rho_r).abs() } else { terms }
    }
}

impl TermCheck {
    /// Checks the α⁰ rows `rows` (0-based) of `fixture` against `model`'s ideal part, orders 0-3 (the oracle's limit;
    /// the order-4 columns are `nan`), with the scale of [`IdealScale`].
    pub fn ideal_rows(
        &mut self,
        fixture: &Fixture<'_>,
        rows: &[usize],
        model: &dyn HelmholtzModel,
        scale: &IdealScale,
    ) {
        for &row in rows {
            let number = |column: &str| fixture.value(row, column);
            let (Some(t), Some(rho), Some(tau), Some(delta)) =
                (number("T"), number("rhomolar"), number("tau"), number("delta"))
            else {
                self.failures.push(format!("row {row}: T, rhomolar, tau or delta is missing"));
                continue;
            };
            let got = model.ideal(t, rho, Order::Three);
            for (column, i, j) in COLUMNS.into_iter().filter(|&(_, i, j)| i + j <= 3) {
                let factor = math::powi(tau, i as i32) * math::powi(delta, j as i32);
                let scaled = scale.get(t, rho, i, j).max(TERM_FLOOR);
                self.checked += 1;
                match fixture.check_scaled(row, column, got.get(i, j).unwrap_or(f64::NAN) / factor, scaled / factor) {
                    Ok(ratio) => self.headroom = self.headroom.max(ratio),
                    Err(CheckError::Mismatch(m)) => self.failures.push(m.to_string()),
                    Err(e) => self.failures.push(format!("row {row}, {column}: {e:?}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No failures, no report; with failures, the count, the total, the headroom and the first lines only.
    #[test]
    fn report_names_the_failures() {
        let mut check = TermCheck { checked: 30, failures: vec![], headroom: 0.25 };
        assert_eq!(check.report(5), None);
        check.failures = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(check.report(2), Some("3 of 30 entries outside Term (headroom 0.250):\na\nb".into()));
    }

    /// δ-entries have the fixed scales of their exact forms; τ-entries sum |term| at ρ_r, and only A00 adds
    /// |ln(ρ/ρ_r)| (Water's ideal terms, 400 K, ρ = 2ρ_r).
    #[test]
    fn ideal_scale_sums_the_terms() {
        let record = phasekit_core::internal::record(phasekit_core::Registry::embedded().unwrap(), "Water").unwrap();
        let scale = IdealScale::new(&record).unwrap();
        let (t, rho_r) = (400.0, record.eos.rho_reducing);
        assert_eq!([scale.get(t, rho_r, 0, 1), scale.get(t, rho_r, 1, 1), scale.get(t, rho_r, 0, 2)], [1.0, 0.0, 1.0]);
        let sum = |i| scale.parts.iter().map(|p| p.eos().ideal(t, rho_r, Order::Three).get(i, 0).unwrap().abs()).sum();
        let (a00, a10): (f64, f64) = (sum(0), sum(1));
        assert!(a00 > 1.0 && a10 > 1.0, "{a00} {a10}");
        assert_eq!(scale.get(t, rho_r, 0, 0), a00);
        assert!((scale.get(t, 2.0 * rho_r, 0, 0) - a00 - math::ln(2.0)).abs() < 1e-14 * a00);
        assert_eq!(scale.get(t, 2.0 * rho_r, 1, 0), a10);
    }
}
