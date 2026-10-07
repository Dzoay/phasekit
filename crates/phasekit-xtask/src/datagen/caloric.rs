//! The saturated caloric curves h′, h″, s′, s″, u′, u″ (PLAN.md M5.2a; user decisions CC1, CC2): on every piece of the
//! superancillary, h, s and u from the compiled `Parity` EOS at (T, ρ_SA(T)) at the 13 Chebyshev-Lobatto nodes, in the
//! record's own gauge, fitted to degree 12 ([`lobatto_fit`]; CoolProp's L matrix, map 03 §3.3). CoolProp builds them at
//! first use, 45-63 ms per fluid behind a mutex, and never checks the fit (map 03 §6); here every piece is checked at
//! the 12 midpoints between its nodes against class `CaloricFit` ([`FIT_TOL`]). The curves are starting points that the
//! saturation-based flashes polish with the EOS (M7.7, M7.8; CC2).

use std::sync::Arc;

use phasekit_core::internal::{CaloricCurves, CaloricStamp, FluidRecord, Superancillary, clenshaw};
use phasekit_core::{Basis, Density, DomainPolicy, FlashOptions, Fluid, Input, Phase, Temperature, math};

use super::superanc::{DEGREE, lobatto_fit};

/// Class `CaloricFit` (VERIFICATION.md §5): 2e-6 of max(|value|, floor), floor R·T for h and u, R for s.
pub const FIT_TOL: f64 = 2e-6;

/// x of the Chebyshev-Lobatto node k on [−1, 1].
fn node(k: usize) -> f64 {
    math::cos(std::f64::consts::PI * k as f64 / DEGREE as f64)
}

/// One piece's six curves fitted from `values(x)` = [h′, h″, s′, s″, u′, u″] at x in [−1, 1], each checked at the
/// midpoints between nodes against [`FIT_TOL`] of max(|value|, floor): `floors(x)` gives (R·T, R) there.
pub fn fit_piece(
    values: impl Fn(f64) -> Result<[f64; 6], String>,
    floors: impl Fn(f64) -> (f64, f64),
) -> Result<[[f64; 13]; 6], String> {
    let at_nodes = (0..=DEGREE).map(|k| values(node(k))).collect::<Result<Vec<_>, _>>()?;
    let rows: [[f64; 13]; 6] = std::array::from_fn(|c| lobatto_fit(&std::array::from_fn(|k| at_nodes[k][c])));
    for k in 0..DEGREE {
        let x = (node(k) + node(k + 1)) / 2.0;
        let (exact, (energy, entropy)) = (values(x)?, floors(x));
        for (c, row) in rows.iter().enumerate() {
            let floor = if c == 2 || c == 3 { entropy } else { energy };
            let miss = (clenshaw(row, x) - exact[c]).abs() / exact[c].abs().max(floor);
            if miss.is_nan() || miss > FIT_TOL {
                let name = ["h'", "h''", "s'", "s''", "u'", "u''"][c];
                return Err(format!(
                    "the {name} fit misses the EOS by {miss:e} at x = {x} (class CaloricFit {FIT_TOL:e})"
                ));
            }
        }
    }
    Ok(rows)
}

/// The caloric curves of `record` on the pieces of its superancillary `sa`, stamped with the superancillary's stamp
/// and the record's α⁰ offset.
pub fn curves(record: &FluidRecord, sa: &Superancillary) -> Result<CaloricCurves, String> {
    let fit = record.superancillary_fit.ok_or("caloric curves need the superancillary's stamp")?;
    let r = record.eos.gas_constant;
    let fluid = Fluid::new(Arc::new(record.clone().compile().map_err(|e| e.to_string())?));
    let side = |phase| FlashOptions::new().with_phase(phase).with_domain(DomainPolicy::Extrapolate);
    let (liquid, gas) = (side(Phase::Liquid), side(Phase::Gas));
    let mut curves: [Vec<[f64; 13]>; 6] = Default::default();
    for (i, w) in sa.breaks.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let t_of = |x: f64| a + (x + 1.0) * (b - a) / 2.0;
        let values = |x: f64| -> Result<[f64; 6], String> {
            let t = Temperature::new(t_of(x)).map_err(|e| e.to_string())?;
            let state = |rho: f64, opts: &FlashOptions| {
                fluid
                    .flash(Input::dt(Density::molar(rho).map_err(|e| e.to_string())?, t), opts)
                    .map_err(|e| e.to_string())
            };
            let l = state(clenshaw(&sa.curves[0][i], x), &liquid)?;
            let v = state(clenshaw(&sa.curves[1][i], x), &gas)?;
            let m = Basis::Molar;
            Ok([l.h(m), v.h(m), l.s(m), v.s(m), l.u(m), v.u(m)])
        };
        let rows =
            fit_piece(values, |x| (r * t_of(x), r)).map_err(|e| format!("caloric piece {i} [{a}, {b}] K: {e}"))?;
        for (curve, row) in curves.iter_mut().zip(rows) {
            curve.push(row);
        }
    }
    let (a1, a2) = record.eos.offset();
    let curves =
        CaloricCurves { stamp: CaloricStamp { superancillary: fit, a1, a2 }, breaks: sa.breaks.clone(), curves };
    curves.check().map_err(|e| e.to_string())?;
    Ok(curves)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CoolProp has no fit-error check (map 03 §6): a piece whose curve has a kink, |x|, which no degree-12 polynomial
    /// follows to 2e-6, is refused and named; a smooth piece is accepted and reproduced at its nodes.
    #[test]
    fn caloric_fit_error_is_checked() {
        let smooth = |x: f64| Ok([1.0 + x, 2.0 * x * x, 3.0, math::exp(x), 0.5 * x, 7.0]);
        let rows = fit_piece(smooth, |_| (1.0, 1.0)).unwrap();
        assert!((clenshaw(&rows[3], 0.3) - math::exp(0.3)).abs() < 1e-12); // degree-12 interpolation of e^x
        let kink = |x: f64| Ok([1.0, 1.0, 1.0, x.abs(), 1.0, 1.0]);
        let err = fit_piece(kink, |_| (1.0, 1.0)).unwrap_err();
        assert!(err.starts_with("the s'' fit misses the EOS by "), "{err}");
        let failing = |_: f64| Err::<[f64; 6], String>("no state".into());
        assert_eq!(fit_piece(failing, |_| (1.0, 1.0)), Err("no state".into()));
    }
}
