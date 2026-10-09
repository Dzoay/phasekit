//! Checking the multiprecision saturation files (VERIFICATION.md §3.4, §5; PLAN.md M6.4), shared by the corpus test
//! (the core subset's fastchebpure files) and the nightly sweep (all of them). The superancillary and the VLE are each
//! compared with the multiprecision values (user decision SV1): the superancillary within class `SaFit` (4 × the
//! point's own SA/mp misfit), the VLE at the row's T, seeded with the superancillary's densities, within `SatMp`, whose
//! bound near Tc carries the double-precision VLE's conditioning (NC1). T, ρ′ and ρ″ are multiprecision; the reference
//! p is the vapour side's at the row's (T, ρ″), as the files' own p carries a double-precision liquid side's rounding
//! (DP1).

use phasekit_core::internal::{FluidRecord, vle_at_t};
use phasekit_core::{HelmholtzModel, Order, SatPair};

use crate::eos::Majorants;
use crate::fixture::Kind;
use crate::register::{DIVERGENCES, exempt_at};
use crate::term::{IdealScale, report};
use crate::{Fixture, ToleranceClass};

/// Below this Θ = (Tc − T)/Tc the VLE is not compared: `SatMp`'s bound on ρ exceeds 1e-3 there (4·ε·Θ^−1.5), and
/// measured at M6.4 over all 130 fluids the VLE answers every row above it but refuses some below it (up to Θ = 6e-9).
pub const THETA_VLE_MIN: f64 = 1e-8;

/// p at (T, ρ) from the model's total order-2 bundle, p = ρRT·A01; NaN if the model refuses the point.
pub fn pressure(eos: &dyn HelmholtzModel, t: f64, rho: f64) -> f64 {
    let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
    d.bundle().map_or(f64::NAN, |b| rho * eos.gas_constant() * t * b.a01)
}

/// `SatMp`'s carried scale μ at the liquid (T, ρ′) of `record` (NC2): the `Term` majorants of its g/RT over g/RT.
pub fn carried_scale(record: &FluidRecord, ideal: &IdealScale, eos: &dyn HelmholtzModel, t: f64, rho_l: f64) -> f64 {
    let m = Majorants::at(record, ideal, t, rho_l);
    let d = eos.ideal(t, rho_l, Order::Two) + eos.residual(t, rho_l, Order::Two);
    d.bundle().map_or(f64::NAN, |b| ToleranceClass::sat_mp_scale(m.m00, m.m01, b.a00, b.a01))
}

/// The running result of a saturation check.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaturationCheck {
    /// Rows whose superancillary was compared (p, ρ′, ρ″ each).
    pub superancillary: usize,
    /// Rows whose VLE answer was compared (p, ρ′, ρ″ each).
    pub vle: usize,
    /// Of the rows whose superancillary was compared, those below [`THETA_VLE_MIN`], where the VLE is not compared.
    pub near_critical: usize,
    /// Of those rows, the ones above [`THETA_VLE_MIN`] where the file's ρ′ equals its ρ″: one phase, so no VLE to
    /// compare. Chlorine's last row is one (Θ = 2.45e-7, between rows with a 5 % split, its own superancillary 4 % off).
    pub unsplit: usize,
    /// Rows at Θ = 0 an ulp beyond the superancillary's range (the critical point itself).
    pub beyond: usize,
    /// Cells the register exempts (DIV-0016), counted, not compared.
    pub exempt: usize,
    /// The largest measured conditioning constants of the VLE near Tc (Θ < 1e-2): |Δρ/ρ|/(ε·Θ^−1.5·μ) over ρ′ and ρ″,
    /// and |Δp/p|/(ε·μ/Θ); `SatMp` allows 4 of each (NC1, NC2).
    pub conditioning: [f64; 2],
    /// One line per disagreement.
    pub failures: Vec<String>,
}

impl SaturationCheck {
    /// Checks every row of `fixture`, a `checkpoints/v1` file of `name`, against the record's superancillary and the
    /// VLE of `eos` (the record compiled).
    pub fn rows(&mut self, fixture: &Fixture<'_>, name: &str, record: &FluidRecord, eos: &dyn HelmholtzModel) {
        let Some(curve) = record.superancillary_curve() else {
            self.failures.push(format!("{name}: no superancillary"));
            return;
        };
        let (t_min, t_max) = curve.t_range();
        let Ok(ideal) = IdealScale::new(record) else {
            self.failures.push(format!("{name}: no ideal-gas scale"));
            return;
        };
        let columns = ["Tc", "T", "p", "rhoL", "rhoV", "p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"];
        for row in 0..fixture.rows().len() {
            let values = columns.iter().map(|c| fixture.value(row, c)).collect::<Option<Vec<f64>>>();
            let Some(&[tc, t, p_file, rho_l, rho_v, p_ratio, rho_l_ratio, rho_v_ratio]) = values.as_deref() else {
                self.failures.push(format!("{name} row {row}: a missing value"));
                continue;
            };
            let theta = (tc - t) / tc;
            let p = pressure(eos, t, rho_v);
            let at = format!("{name} row {row} (T {t} K, Θ {theta:.2e})");
            // A file's first row can sit an ulp or two below the superancillary's triple point (as in tests/sat.rs).
            let t_sa = if t < t_min && t >= t_min * (1.0 - 4.0 * f64::EPSILON) { t_min } else { t };
            let sa = match curve.at_t(t_sa) {
                Ok(sa) => sa,
                Err(_) if t > t_max && theta < f64::EPSILON => {
                    self.beyond += 1;
                    continue;
                }
                Err(e) => {
                    self.failures.push(format!("{at}: superancillary {e:?}"));
                    continue;
                }
            };
            // SaFit, the misfit the file records (its superancillary's p against the derived p, DP1).
            let sa_fit = [
                ("p", sa.dew.p, p, p_ratio * p_file / p),
                ("rhoL", sa.bubble.rho, rho_l, rho_l_ratio),
                ("rhoV", sa.dew.rho, rho_v, rho_v_ratio),
            ];
            for (column, got, want, ratio) in sa_fit {
                let bound = ToleranceClass::sa_fit(ratio, want);
                let within = (got - want).abs() <= bound; // false for NaN too
                if !within {
                    self.failures.push(format!("{at} superancillary {column}: {got} against {want} (bound {bound:e})"));
                }
            }
            self.superancillary += 1;
            if theta < THETA_VLE_MIN {
                self.near_critical += 1;
                continue;
            }
            if rho_l == rho_v {
                self.unsplit += 1;
                continue;
            }
            match vle_at_t(eos, t, (sa.bubble.rho, sa.dew.rho)) {
                Ok(sat) => {
                    let mu = carried_scale(record, &ideal, eos, t, rho_l);
                    self.vle_row((name, &at), &sat, (t, theta, mu), [p, rho_l, rho_v]);
                    self.vle += 1;
                }
                Err(e) => self.failures.push(format!("{at}: VLE {e:?}")),
            }
        }
    }

    /// One VLE answer of `name` against `[p, ρ′, ρ″]` within `SatMp` at (T, Θ, μ), but for the cells the register
    /// exempts.
    fn vle_row(
        &mut self,
        (name, at): (&str, &str),
        sat: &SatPair,
        (t, theta, mu): (f64, f64, f64),
        [p, rho_l, rho_v]: [f64; 3],
    ) {
        let checks = [
            ("p", sat.dew.p, p, ToleranceClass::sat_mp_pressure(theta, mu, p)),
            ("rhoL", sat.bubble.rho, rho_l, ToleranceClass::sat_mp_density(theta, mu, rho_l)),
            ("rhoV", sat.dew.rho, rho_v, ToleranceClass::sat_mp_density(theta, mu, rho_v)),
        ];
        for (column, got, want, bound) in checks {
            let within = (got - want).abs() <= bound; // false for NaN too
            if exempt_at(DIVERGENCES, name, Kind::Checkpoints, column, t).is_some() {
                self.exempt += 1;
            } else if !within {
                self.failures.push(format!("{at} VLE {column}: {got} against {want} (bound {bound:e})"));
            }
        }
        if theta < 1e-2 {
            let density = (sat.bubble.rho / rho_l - 1.0).abs().max((sat.dew.rho / rho_v - 1.0).abs());
            let measured = [
                density / (f64::EPSILON * phasekit_core::math::powf(theta, -1.5) * mu),
                (sat.dew.p / p - 1.0).abs() / (f64::EPSILON * mu / theta),
            ];
            self.conditioning = [self.conditioning[0].max(measured[0]), self.conditioning[1].max(measured[1])];
        }
    }

    /// `None` without failures, else their count, the rows checked and the first `shown` failure lines.
    pub fn report(&self, shown: usize) -> Option<String> {
        let rows = self.superancillary + self.beyond;
        report(&self.failures, rows, 0.0, "SaFit (superancillary) / SatMp (VLE)", shown)
    }
}
