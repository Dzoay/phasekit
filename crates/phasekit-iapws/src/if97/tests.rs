use phasekit_core::{Basis, DomainError, Error, FlashOptions, Input, Pair, Phase, Quantity, RootPolicy};

use super::*;

/// Eqs. (8) to (10): at the triple point, 273.16 K and 611.657 Pa, region 1's liquid has u = s = 0, and so
/// h = pv = 0.611783 J/kg, printed to six digits. u and h hold within half a unit of that digit (u = −1.3e-7 J/kg); s
/// only to −6.16e-5 J/(kg K), which v8.0.0's `IF97::Water` gives too (−6.161029e-5 at its saturated liquid, measured at
/// M14.2), so it is held within 1e-4.
#[test]
fn the_triple_point_liquid_is_the_zero_of_u_and_s() {
    let state = region1(273.16, 611.657).unwrap();
    assert!(state.u(Basis::Mass).abs() <= 5e-7, "u = {}", state.u(Basis::Mass));
    assert!(state.s(Basis::Mass).abs() <= 1e-4, "s = {}", state.s(Basis::Mass));
    assert!((state.h(Basis::Mass) - 0.611_783).abs() <= 5e-7, "h = {}", state.h(Basis::Mass));
}

/// Region 1 at its bounds: 273.15 K, 623.15 K and 100 MPa are inside, a step beyond each is refused, as are
/// non-positive and non-finite inputs; above the critical pressure the liquid is supercritical.
#[test]
fn region_1_takes_its_box_and_refuses_beyond() {
    for (t, p) in [(T_MIN, 1e5), (T_MAX_1, 30e6), (300.0, P_MAX), (300.0, P_C)] {
        assert!(region1(t, p).is_ok(), "({t}, {p})");
    }
    let below = DomainError::BelowMinTemperature { t: 273.0, t_min: T_MIN };
    let above = DomainError::AboveMaxTemperature { t: 623.2, t_max: T_MAX_1 };
    let over = DomainError::AboveMaxPressure { p: 100.1e6, p_max: P_MAX };
    assert_eq!(region1(273.0, 1e5).unwrap_err(), below.into());
    assert_eq!(region1(623.2, 30e6).unwrap_err(), above.into());
    assert_eq!(region1(300.0, 100.1e6).unwrap_err(), over.into());
    assert_eq!(region1(300.0, 0.0).unwrap_err(), Error::InvalidInput { quantity: Quantity::PRESSURE, value: 0.0 });
    assert!(matches!(region1(f64::NAN, 1e5), Err(Error::InvalidInput { quantity: Quantity::TEMPERATURE, .. })));
    assert_eq!(region1(300.0, P_C).unwrap().phase(), Phase::Liquid);
    assert_eq!(region1(300.0, 22.1e6).unwrap().phase(), Phase::SupercriticalLiquid);
    assert_eq!(region1(300.0, 3e6).unwrap().key(), KEY);
}

/// Region 2 at its bounds: 273.15 K, 1073.15 K and 100 MPa are inside, a step beyond each is refused. Its phase is gas
/// to T_c, the critical temperature included, and above it supercritical gas to p_c, the critical pressure included, and
/// supercritical beyond, where only a direct call reaches it (the `Fluid` stops at 623.15 K).
#[test]
fn region_2_takes_its_box_and_names_its_phase() {
    for (t, p) in [(T_MIN, 600.0), (T_MAX_2, 1e5), (1000.0, P_MAX)] {
        assert!(region2(t, p).is_ok(), "({t}, {p})");
    }
    let above = DomainError::AboveMaxTemperature { t: 1073.2, t_max: T_MAX_2 };
    let over = DomainError::AboveMaxPressure { p: 100.1e6, p_max: P_MAX };
    assert_eq!(region2(1073.2, 1e5).unwrap_err(), above.into());
    assert_eq!(region2(1000.0, 100.1e6).unwrap_err(), over.into());
    for (t, p, phase) in [
        (T_C, 1e6, Phase::Gas),
        (700.0, 1e6, Phase::SupercriticalGas),
        (700.0, P_C, Phase::SupercriticalGas),
        (700.0, 30e6, Phase::Supercritical),
    ] {
        assert_eq!(region2(t, p).unwrap().phase(), phase, "({t}, {p})");
    }
}

/// Eqs. (30) and (31) come from one implicit equation, so each inverts the other (R7-97(2012) §8.2), here to 1e-11
/// over the line (at T_c Eq. (31)'s square root of a near-zero discriminant leaves 1.8e-12); their ends: 273.15 K to T_c, 611.213 Pa to p_c, and a step beyond is refused.
#[test]
fn the_saturation_equations_invert_each_other() {
    for k in 0..=40 {
        let t = T_MIN + f64::from(k) * (T_C - T_MIN) / 40.0;
        let p = saturation_pressure(t).unwrap();
        let back = saturation_temperature(p.min(P_C)).unwrap();
        assert!((back - t).abs() <= 1e-11 * t, "{t} K: {p} Pa gives {back} K");
    }
    assert!((saturation_pressure(T_C).unwrap() / P_C - 1.0).abs() < 1e-8);
    let p_min = saturation_pressure(T_MIN).unwrap();
    assert!((p_min - 611.213).abs() < 5e-4, "the release rounds it to 611.213 Pa: {p_min}");
    assert!((saturation_temperature(p_min).unwrap() - T_MIN).abs() < 1e-9);
    let cold = DomainError::BelowMinTemperature { t: 273.1, t_min: T_MIN };
    let hot = DomainError::AboveMaxTemperature { t: 647.1, t_max: T_C };
    assert_eq!(saturation_pressure(273.1).unwrap_err(), cold.into());
    assert_eq!(saturation_pressure(647.1).unwrap_err(), hot.into());
    let low = DomainError::BelowMinPressure { p: 611.0, p_min };
    let high = DomainError::AboveMaxPressure { p: 22.1e6, p_max: P_C };
    assert_eq!(saturation_temperature(611.0).unwrap_err(), low.into());
    assert_eq!(saturation_temperature(22.1e6).unwrap_err(), high.into());
    assert!(matches!(saturation_temperature(-1.0), Err(Error::InvalidInput { quantity: Quantity::PRESSURE, .. })));
}

/// IF97 water as a `Fluid`: on the line PT takes the side nearer `Nearest`'s density, the liquid on a tie and under
/// `Stable`, and is `Ambiguous` under `Strict`; 623.15 K is its top, so vapour above it is refused though region 2 holds
/// there, and so is PQ above its saturation pressure; its limits say so.
#[test]
fn the_fluid_takes_the_line_by_its_roots_and_stops_at_623_k() {
    let fluid = water().unwrap();
    let (t, q) = (400.0, 0.5);
    let p = saturation_pressure(t).unwrap();
    let rho = |state: State| state.rho(Basis::Molar);
    let (liquid, vapour) = (rho(region1(t, p).unwrap()), rho(region2(t, p).unwrap()));
    let on_line = Input::new(Pair::PT, p, t, Basis::Molar).unwrap();
    let side = |roots| fluid.flash(on_line, &FlashOptions::new().with_roots(roots)).map(rho);
    for (target, want) in
        [(2.0 * vapour, vapour), (liquid / 2.0, vapour), (0.9 * liquid, liquid), (f64::INFINITY, liquid)]
    {
        assert_eq!(side(RootPolicy::Nearest(target)), Ok(want), "nearest {target}");
    }
    assert_eq!(side(RootPolicy::Stable), Ok(liquid));
    assert!(matches!(side(RootPolicy::Strict), Err(Error::Ambiguous { .. })));
    let hot = DomainError::AboveMaxTemperature { t: 630.0, t_max: T_MAX_1 };
    assert_eq!(fluid.state(Input::new(Pair::PT, 1e5, 630.0, Basis::Molar).unwrap()).unwrap_err(), hot.into());
    let p_top = saturation_pressure(T_MAX_1).unwrap();
    let pq = |p: f64| fluid.state(Input::new(Pair::PQ, p, q, Basis::Molar).unwrap()).map(|s| s.t());
    assert_eq!(pq(p_top), Ok(T_MAX_1));
    assert_eq!(pq(20e6).unwrap_err(), DomainError::AboveMaxPressure { p: 20e6, p_max: p_top }.into());
    let limits = fluid.model().limits().unwrap();
    assert_eq!(
        (limits.t_min(), limits.t_max(), limits.p_max(), limits.t_triple()),
        (T_MIN, T_MAX_1, P_MAX, Some(273.16))
    );
}
