//! The cubic superancillary (PLAN.md M11.5; map 06 U4; ROT-219): its saturations are phase equilibria of the cubic,
//! it refuses outside its range, and the core's QT flash takes it there and its own VLE below.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, DataSet, DomainError, Error, FlashOptions, Input, Order, Pair, Registry, SaturationCurve, Strategy, math,
};
use phasekit_cubic::{Alpha, CubicSaturation, Kind, R_CODATA, Residual, fluid};

/// SRK and PR of n-Propane, Water and R134a with Soave's alpha, on both datasets' Ω.
fn cubics() -> Vec<(String, Residual)> {
    let mut out = Vec::new();
    for (name, t_c, p_c, omega) in [
        ("n-Propane", 369.89, 4.2512e6, 0.1521),
        ("Water", 647.096, 2.2064e7, 0.3443),
        ("R134a", 374.21, 4.059_28e6, 0.327_05),
    ] {
        for kind in [Kind::Srk, Kind::Pr] {
            for data in [DataSet::Corrected, DataSet::Parity] {
                let alpha = Alpha::Soave(kind.soave_m(omega).unwrap());
                out.push((format!("{kind:?} {name} {data:?}"), Residual::new(kind, data, alpha, t_c, p_c)));
            }
        }
    }
    out
}

/// ROT-219, map 06 U4: the superancillary's saturations are the cubic's phase equilibria. At 40 temperatures across
/// each curve's range, up to 1e-3 of it below its top, the cubic's own p at (T, ρ′) and at (T, ρ″) is the
/// superancillary's p within 1e-12 of ρRT on each side (the liquid's p = ρ′RT(1 + A01) is a cancellation: 3e-7 Pa from
/// terms of 1e7 Pa at SRK n-Propane's 72 K), and g′ = g″ within 1e-12 times the liquid's 1 + |α^r| + |A01| (1.0e-12 RT
/// at SRK Water's 149 K, where those are about 20): g/RT = α^r + A01 + ln ρ plus a part of T alone. At 10 more
/// temperatures up to 1e-6 below the top the bound is 1e-10: there ∂p/∂ρ → 0 and the expansions' densities carry the
/// error (1.25e-11 of ρRT at SRK n-Propane's 1e-6, the two sides' p symmetric about the superancillary's to 1e-14).
#[test]
fn superancillary_is_the_cubics_phase_equilibrium() {
    for (what, r) in cubics() {
        let curve = CubicSaturation::new(r).unwrap();
        let (lo, hi) = curve.t_range();
        assert!(lo > 0.0 && hi > lo, "{what}: {lo} {hi}");
        let far = (0..40).map(|k| (lo + (hi - lo) * (1.0 - 1e-3) * f64::from(k) / 39.0, 1e-12));
        let near = (1..=10).map(|k| (hi - (hi - lo) * math::powi(10.0, -3 - k / 4) * 0.999, 1e-10));
        for (t, bound) in far.chain(near) {
            let sat = curve.at_t(t).unwrap();
            assert!(sat.is_pure() && sat.bubble.t == t && sat.bubble.rho > sat.dew.rho, "{what} at {t}");
            let side = |rho: f64| {
                let d = r.derivs(t, rho, Order::One);
                let (ar, a01) = (d.get(0, 0).unwrap(), d.get(0, 1).unwrap());
                let (rt, size) = (rho * R_CODATA * t, 1.0 + ar.abs() + a01.abs());
                (rt * (1.0 + a01), rt, ar + a01 + math::ln(rho), size)
            };
            let ((p_l, rt_l, g_l, size), (p_v, rt_v, g_v, _)) = (side(sat.bubble.rho), side(sat.dew.rho));
            let p = sat.bubble.p;
            let (dl, dv) = ((p_l - p).abs() / rt_l, (p_v - p).abs() / rt_v);
            assert!(dl <= bound && dv <= bound, "{what} at {t}: {p} {p_l} {p_v} ({dl:e}, {dv:e})");
            assert!((g_l - g_v).abs() <= bound * size, "{what} at {t}: Δg/RT {:e}", g_l - g_v);
        }
    }
}

/// `at_p` inverts `at_t`: at each `at_t` pressure it returns that T within a few ulp, and the given p on both sides.
#[test]
fn at_p_inverts_at_t() {
    for (what, r) in cubics() {
        let curve = CubicSaturation::new(r).unwrap();
        let (lo, hi) = curve.t_range();
        for k in 0..20 {
            let t = lo + (hi - lo) * (1.0 - 1e-6) * f64::from(k) / 19.0;
            let p = curve.at_t(t).unwrap().bubble.p;
            let back = curve.at_p(p).unwrap();
            assert!((back.bubble.t / t - 1.0).abs() <= 1e-12, "{what}: {t} back as {}", back.bubble.t);
            assert_eq!((back.bubble.p, back.dew.p), (p, p), "{what}");
        }
    }
}

/// Outside its range the curve refuses with the bound (the core never asks it there, D6), and on its pressure bounds it
/// answers with the range's ends; van der Waals has none.
#[test]
fn the_curve_refuses_outside_its_range() {
    let (what, r) = cubics().remove(0);
    let curve = CubicSaturation::new(r).unwrap();
    let (lo, hi) = curve.t_range();
    assert!(matches!(curve.at_t(0.99 * lo), Err(Error::Domain(DomainError::BelowMinTemperature { .. }))), "{what}");
    assert!(matches!(curve.at_t(1.0001 * hi), Err(Error::Domain(DomainError::AboveMaxTemperature { .. }))), "{what}");
    let (p_lo, p_hi) = (curve.at_t(lo).unwrap().bubble.p, curve.at_t(hi).unwrap().bubble.p);
    assert!(matches!(curve.at_p(0.5 * p_lo), Err(Error::Domain(DomainError::BelowMinPressure { .. }))), "{what}");
    assert!(matches!(curve.at_p(2.0 * p_hi), Err(Error::Domain(DomainError::AboveMaxPressure { .. }))), "{what}");
    for (p, t) in [(p_lo, lo), (p_hi, hi)] {
        assert_eq!(curve.at_p(p).map(|s| (s.bubble.t, s.dew.p)).ok(), Some((t, p)), "{what}");
    }
    assert!(curve.at_t(f64::NAN).is_err() && curve.at_p(f64::NAN).is_err());
    let vdw = Residual::new(Kind::Vdw, DataSet::Corrected, Alpha::One, 369.89, 4.2512e6);
    assert_eq!(CubicSaturation::new(vdw), None);
}

/// The core's QT flash on PR n-Propane takes the superancillary (`Strategy::Superancillary`), its p the curve's bit for
/// bit, down to the fluid's T_min: the curve reaches T̃ = 0.1 T̃_c, about 0.18 T_c, and n-Propane's T_min is 0.23 T_c.
#[test]
fn the_flash_takes_the_curve() {
    let source = Registry::embedded().unwrap().get("n-Propane").unwrap();
    let propane = fluid(source, Kind::Pr, None, DataSet::Corrected).unwrap();
    let t_min = propane.model().limits().unwrap().t_min();
    let package = (propane.model() as &dyn core::any::Any).downcast_ref::<phasekit_core::PureFluid>().unwrap();
    let curve = package.saturation().unwrap().unwrap();
    assert!(curve.t_range().0 < t_min, "{:?} against T_min {t_min}", curve.t_range());
    for t in [t_min, 200.0, 300.0, 369.0] {
        let state =
            propane.flash(Input::new(Pair::QT, 0.0, t, Basis::Molar).unwrap(), &FlashOptions::default()).unwrap();
        assert_eq!(state.path().strategy, Strategy::Superancillary, "{t}");
        assert_eq!(state.p().to_bits(), curve.at_t(t).unwrap().bubble.p.to_bits(), "{t}");
    }
}
