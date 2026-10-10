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
    /// Rows at Θ ≤ 0 (the critical point itself, or an ulp above it) beyond the superancillary's range.
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
        let (t_min, _) = curve.t_range();
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
            // A file's first row can sit an ulp or two below the superancillary's triple point (as in tests/sat.rs):
            // up to 4 ulp below (positive doubles order as their bits), it is evaluated at the curve's start.
            let below = t_min.to_bits().saturating_sub(t.to_bits());
            let t_sa = if below <= 4 { t.max(t_min) } else { t };
            let sa = match curve.at_t(t_sa) {
                Ok(sa) => sa,
                // A file's last row is at its Tc, or an ulp above it, which can lie an ulp beyond the curve's end.
                Err(_) if theta <= 0.0 => {
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

/// The 40 temperatures [`check_rescaling`] samples, evenly spaced from `lo` to Θ = 1e-3 below `tc`, each with its Θ.
fn rescaling_points(lo: f64, tc: f64) -> impl Iterator<Item = (f64, f64)> {
    (0..40_u32).map(move |k| {
        let t = lo + (tc * (1.0 - 1e-3) - lo) * f64::from(k) / 39.0;
        (t, (tc - t) / tc)
    })
}

/// The exact rescaling of a superancillary (ARCHITECTURE.md §8, E14; DIV-0001, DIV-0003). Saturation is invariant in
/// (τ, δ), so a correction of R or ρ_r scales ρ′ and ρ″ by ρ_r′/ρ_r and p by (R′/R)·(ρ_r′/ρ_r), and `Corrected` scales
/// the superancillary fitted to `Parity` by those factors. On 40 temperatures from the curve's lower end to Θ = 1e-3,
/// each `SatMp` (Θ, and μ of the corrected liquid): the corrected EOS's VLE equals the factors times the Parity EOS's,
/// and the rescaled superancillary equals the corrected VLE. Returns the points checked, or the failures.
pub fn check_rescaling(parity: &FluidRecord, corrected: &FluidRecord) -> Result<usize, Vec<String>> {
    let name = &corrected.name;
    let curves = parity.superancillary_curve().zip(corrected.superancillary_curve());
    let models = parity.clone().compile().ok().zip(corrected.clone().compile().ok());
    let (Some((curve_p, curve_c)), Some((model_p, model_c)), Ok(ideal)) = (curves, models, IdealScale::new(corrected))
    else {
        return Err(vec![format!("{name}: no superancillary, model or ideal-gas scale")]);
    };
    let rho = corrected.eos.rho_reducing / parity.eos.rho_reducing;
    let p = rho * corrected.eos.gas_constant / parity.eos.gas_constant;
    let (lo, tc) = curve_c.t_range();
    let (mut failures, mut checked) = (Vec::new(), 0);
    for (t, theta) in rescaling_points(lo, tc) {
        let solve = |curve: &dyn phasekit_core::SaturationCurve, eos: &dyn HelmholtzModel| {
            let sa = curve.at_t(t).ok()?;
            Some((sa, vle_at_t(eos, t, (sa.bubble.rho, sa.dew.rho)).ok()?))
        };
        let (Some((_, vle_p)), Some((sa_c, vle_c))) =
            (solve(&*curve_p, model_p.eos()), solve(&*curve_c, model_c.eos()))
        else {
            failures.push(format!("{name} at {t} K: no superancillary or VLE answer"));
            continue;
        };
        let mu = carried_scale(corrected, &ideal, model_c.eos(), t, vle_c.bubble.rho);
        let pairs = [
            ("VLE p", vle_c.dew.p, vle_p.dew.p * p, false),
            ("VLE rhoL", vle_c.bubble.rho, vle_p.bubble.rho * rho, true),
            ("VLE rhoV", vle_c.dew.rho, vle_p.dew.rho * rho, true),
            ("superancillary p", sa_c.dew.p, vle_c.dew.p, false),
            ("superancillary rhoL", sa_c.bubble.rho, vle_c.bubble.rho, true),
            ("superancillary rhoV", sa_c.dew.rho, vle_c.dew.rho, true),
        ];
        for (what, got, want, density) in pairs {
            let bound = if density {
                ToleranceClass::sat_mp_density(theta, mu, want)
            } else {
                ToleranceClass::sat_mp_pressure(theta, mu, want)
            };
            let within = (got - want).abs() <= bound; // false for NaN too
            if !within {
                failures.push(format!("{name} at {t} K, {what}: {got} against {want} (bound {bound:e})"));
            }
        }
        checked += 1;
    }
    if failures.is_empty() { Ok(checked) } else { Err(failures) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::{DataSet, Registry};

    const WATER: (&str, &str) = crate::fixture!("mp/fastchebpure-2026.06.02-v2/Water.csv");

    /// Water's file, its header and its rows as `[f64; 8]` (the columns after the name).
    fn water() -> (String, Vec<[f64; 8]>) {
        let header: String = WATER.1.lines().filter(|l| l.starts_with('#')).map(|l| format!("{l}\n")).collect();
        let rows = WATER.1.lines().filter(|l| !l.starts_with('#')).map(|l| {
            let cells: Vec<f64> = l.split(',').skip(1).map(|c| c.parse().unwrap()).collect();
            <[f64; 8]>::try_from(cells).unwrap()
        });
        (header, rows.collect())
    }

    /// Checks `rows` of `name` as a `checkpoints/v1` file under `header`.
    fn check(name: &str, header: &str, rows: &[[f64; 8]]) -> SaturationCheck {
        let body: String = rows.iter().map(|r| format!("{name},{}\n", r.map(|v| format!("{v:?}")).join(","))).collect();
        let text = format!("{header}{body}");
        let fixture = Fixture::parse("synthetic.csv", &text).unwrap();
        let mut record = phasekit_core::internal::record(Registry::embedded().unwrap(), name).unwrap();
        record.apply(DataSet::Parity).unwrap();
        let fluid = record.clone().compile().unwrap();
        let mut check = SaturationCheck::default();
        check.rows(&fixture, name, &record, fluid.eos());
        check
    }

    /// Water's rows at its triple point (an ulp below the superancillary's), at Θ ≈ 0.3, in the near-critical range
    /// (Θ ≈ 1e-4), below Θ = 1e-8, and at Tc; and a row moved past the curve's end, its Tc an ulp below its T (Θ < 0,
    /// as the last rows of CarbonDioxide, HFE143m and R125).
    fn picked() -> (String, [[f64; 8]; 6]) {
        let (header, rows) = water();
        let theta = |r: &[f64; 8]| (r[0] - r[1]) / r[0];
        let first = |lo: f64, hi: f64| *rows.iter().find(|r| (lo..hi).contains(&theta(r))).unwrap();
        let last = *rows.last().unwrap();
        let record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
        let end = record.superancillary_curve().unwrap().t_range().1 * (1.0 + 2.0 * f64::EPSILON);
        let mut past = last;
        (past[0], past[1]) = (end, end.next_up());
        (header, [rows[0], first(0.25, 0.35), first(5e-5, 2e-4), first(1e-12, 1e-8), last, past])
    }

    /// A faithful file passes, and every row is counted where it belongs: the triple point an ulp below the curve's
    /// end is evaluated at its end, Tc itself lies beyond it, and below Θ = 1e-8 the VLE is not compared. The
    /// near-critical row's conditioning is its error over ε·Θ^−1.5·μ and ε·μ/Θ.
    #[test]
    fn a_faithful_file_passes_and_every_row_is_counted() {
        let (header, rows) = picked();
        let check = check("Water", &header, &rows);
        assert_eq!(check.report(5), None);
        let counts = (check.superancillary, check.vle, check.near_critical, check.unsplit, check.beyond, check.exempt);
        assert_eq!(counts, (5, 3, 2, 0, 1, 0));
        let r = rows[2];
        let mut record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
        record.apply(DataSet::Parity).unwrap();
        let fluid = record.clone().compile().unwrap();
        let sa = record.superancillary_curve().unwrap().at_t(r[1]).unwrap();
        let sat = vle_at_t(fluid.eos(), r[1], (sa.bubble.rho, sa.dew.rho)).unwrap();
        let theta = (r[0] - r[1]) / r[0];
        let mu = carried_scale(&record, &IdealScale::new(&record).unwrap(), fluid.eos(), r[1], r[3]);
        let density = (sat.bubble.rho / r[3] - 1.0).abs().max((sat.dew.rho / r[4] - 1.0).abs());
        let p = pressure(fluid.eos(), r[1], r[4]);
        let want = [
            density / (f64::EPSILON * phasekit_core::math::powf(theta, -1.5) * mu),
            (sat.dew.p / p - 1.0).abs() / (f64::EPSILON * mu / theta),
        ];
        assert_eq!(check.conditioning, want);
        assert!(want.iter().all(|c| *c > 0.0 && *c < 0.5), "{want:?}");
    }

    /// A misfit is reported where it is: ρ″ 1e-9 off fails both the superancillary (`SaFit`) and the VLE (`SatMp`), in
    /// p too, as the reference p is derived from ρ″ (DP1);
    /// ρ′ 1e-12 off fails the superancillary alone; a row past the curve's end that is not at its Tc, or 5 ulp below
    /// its start (4 is evaluated at its start), is a failure; and the report counts every row checked, the one at Tc
    /// beyond the curve too.
    #[test]
    fn misfits_are_reported_where_they_are() {
        let (header, rows) = picked();
        let mut bad = rows;
        bad[1][4] *= 1.0 + 1e-9;
        bad[2][3] *= 1.0 + 1e-12;
        bad[5][0] *= 1.0 + 1e-6;
        let t_min = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water")
            .unwrap()
            .superancillary_curve()
            .unwrap()
            .t_range()
            .0;
        let [mut four, mut five] = [rows[0]; 2];
        (four[1], five[1]) = (f64::from_bits(t_min.to_bits() - 4), f64::from_bits(t_min.to_bits() - 5));
        let check = check("Water", &header, &[bad[0], bad[1], bad[2], bad[3], bad[4], bad[5], five, rows[5], four]);
        let what: Vec<&str> = check.failures.iter().map(|f| f.split(": ").next().unwrap_or("")).collect();
        assert_eq!(check.failures.len(), 6, "{:?}", check.failures);
        let ends = ["superancillary rhoV", "VLE p", "VLE rhoV", "superancillary rhoL"];
        assert!(what.iter().zip(ends).all(|(w, end)| w.ends_with(end)), "{what:?}");
        assert!(
            check.failures[4].contains("superancillary Domain") && check.failures[5].contains("Domain"),
            "{what:?}"
        );
        let report = check.report(9).unwrap();
        assert!(report.starts_with("6 of 7 entries"), "{report}");
    }

    /// The superancillary's p is measured against the p derived from (T, ρ″) (DP1), its bound from the misfit the file
    /// records against the file's own p: a file that claims the superancillary's p exact there fails it.
    #[test]
    fn a_superancillary_p_misfit_is_measured_against_the_derived_p() {
        let (header, rows) = picked();
        let mut row = rows[1];
        let mut record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
        record.apply(DataSet::Parity).unwrap();
        let p = pressure(record.clone().compile().unwrap().eos(), row[1], row[4]);
        let misfit = (record.superancillary_curve().unwrap().at_t(row[1]).unwrap().dew.p / p - 1.0).abs();
        assert!(misfit > 2e-14 && (row[2] / p - 1.0).abs() > 1e-15, "{misfit:e}");
        assert_eq!(check("Water", &header, &[row]).report(5), None);
        row[5] = p / row[2];
        let check = check("Water", &header, &[row]);
        assert_eq!(check.failures.len(), 1, "{:?}", check.failures);
        assert!(check.failures[0].contains("superancillary p: "), "{:?}", check.failures);
    }

    /// The rescaling check's 40 points: from the curve's lower end (Θ = 0.5 for 100 K under Tc = 200 K) to Θ = 1e-3,
    /// evenly spaced (the 14th a third of the way).
    #[test]
    fn rescaling_samples_forty_points_up_to_theta_1e_3() {
        let points: Vec<(f64, f64)> = rescaling_points(100.0, 200.0).collect();
        assert_eq!((points.len(), points[0]), (40, (100.0, 0.5)));
        let (t, theta) = points[39];
        assert!((t - 199.8).abs() < 1e-12 && (theta - 1e-3).abs() < 1e-15, "{t} {theta}");
        let (t, theta) = points[13];
        assert!((t - (100.0 + 99.8 / 3.0)).abs() < 1e-12 && (theta - 0.333_666_666_666_666_7).abs() < 1e-15);
    }

    /// A row whose ρ′ equals its ρ″ above Θ = 1e-8 is unsplit, its VLE not compared; the register's exempt cells
    /// (DIV-0016: PropyleneGlycol's p and ρ″ from 213 to 227.6028 K) are counted, not compared.
    #[test]
    fn unsplit_rows_and_exempt_cells_are_counted() {
        let (header, rows) = picked();
        let mut unsplit = rows[1];
        unsplit[4] = unsplit[3];
        let curve = |name: &str| {
            let record = phasekit_core::internal::record(Registry::embedded().unwrap(), name).unwrap();
            (record.superancillary_curve().unwrap(), record.compile().unwrap())
        };
        let (water_curve, _) = curve("Water");
        unsplit[7] = water_curve.at_t(unsplit[1]).unwrap().dew.rho / unsplit[3];
        let check_unsplit = check("Water", &header, &[unsplit]);
        assert_eq!((check_unsplit.unsplit, check_unsplit.vle, check_unsplit.report(5)), (1, 0, None));
        let (pg_curve, pg) = curve("PropyleneGlycol");
        let t = 215.0;
        let sa = pg_curve.at_t(t).unwrap();
        let sat = vle_at_t(pg.eos(), t, (sa.bubble.rho, sa.dew.rho)).unwrap();
        let p = pressure(pg.eos(), t, sat.dew.rho);
        let tc = pg_curve.t_range().1;
        let row = [
            tc,
            t,
            p,
            sat.bubble.rho,
            sat.dew.rho,
            sa.dew.p / p,
            sa.bubble.rho / sat.bubble.rho,
            sa.dew.rho / sat.dew.rho,
        ];
        let header = header.replace("Water", "PropyleneGlycol");
        let exempt = check("PropyleneGlycol", &header, &[row, row]);
        assert_eq!((exempt.exempt, exempt.vle, exempt.report(5)), (4, 2, None));
    }
}
