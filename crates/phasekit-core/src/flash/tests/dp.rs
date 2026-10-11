//! DP along the isochore (PLAN.md M7.6).

use super::*;

/// ROT-070 (map 03 §6 row 2): CoolProp's DP ignores an imposed phase. Water at 1.05 ρ″(400 K), 79.81 mol/m³, and
/// psat(400 K), 245.77 kPa, is two-phase at 400 K, Q 0.95231, with or without an imposed gas. Here DP without a
/// hint is that state (T and Q within 1e-9 of CoolProp's), and with `Gas` imposed the vapour at that ρ and p,
/// supercooled below 395 K, whose DT under the same hint gives p back.
#[cfg(feature = "fluids-all")]
#[test]
fn dp_honours_the_phase_hint() {
    let water = embedded("Water");
    let sat = record_of("Water").superancillary_curve().unwrap().at_t(400.0).unwrap();
    let (rho, p) = (1.05 * sat.dew.rho, sat.dew.p);
    let input = Input::dp(Density::molar(rho).unwrap(), Pressure::new(p).unwrap());
    let free = water.state(input).unwrap();
    let q = free.quality();
    assert!(free.phase() == Phase::TwoPhase && (free.t() / 400.0 - 1.0).abs() < 1e-9, "{free:?}");
    assert!(q.is_some_and(|q| (q - 0.952_311_292_382_732_8).abs() < 1e-9), "{q:?}");
    let gas = FlashOptions::new().with_phase(Phase::Gas);
    let imposed = water.flash(input, &gas).unwrap();
    assert!(
        imposed.phase() == Phase::Gas && imposed.t() < 395.0 && (imposed.p() / p - 1.0).abs() < 1e-9,
        "{imposed:?}"
    );
    let back = water.flash(dt(rho, imposed.t()), &gas).unwrap();
    assert!((back.p() / p - 1.0).abs() < 1e-9, "{back:?}");
}

/// M7.6: a pressure input is reproduced within 1e-9 of it (D6), and not beyond.
#[test]
fn a_pressure_is_reproduced_within_1e_9() {
    for p in [1e3, 2.5e5, 1e8] {
        assert!(reproduces_p(p, p * (1.0 + 5e-10)) && reproduces_p(p, p * (1.0 - 5e-10)), "{p}");
        assert!(!reproduces_p(p, p * (1.0 + 2e-9)) && !reproduces_p(p, p * (1.0 - 2e-9)), "{p}");
    }
}

/// M7.6 (D6, user decision 6): DP checks p against the domain under `Enforce`, not under `Extrapolate`. Water at
/// 1.1 of its pmax and 1000 K is `AboveMaxPressure`, and under `Extrapolate` the state at that p, flagged.
#[cfg(feature = "fluids-all")]
#[test]
fn dp_checks_p_against_the_domain() {
    let water = embedded("Water");
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let p_max = record_of("Water").limits.p_max();
    let state = water.flash(pt(1.1 * p_max, 1000.0), &extrapolate).unwrap();
    let input = Input::dp(Density::molar(state.rho(Basis::Molar)).unwrap(), Pressure::new(state.p()).unwrap());
    let refused = water.state(input);
    assert!(matches!(refused, Err(Error::Domain(DomainError::AboveMaxPressure { .. }))), "{refused:?}");
    let back = water.flash(input, &extrapolate).unwrap();
    assert!(back.is_extrapolated() && (back.t() / 1000.0 - 1.0).abs() < 1e-9, "{back:?}");
}

/// M7.6: where a pseudo-pure fluid's VLE fails just below its published Tc (Air from about 131.9 K to 132.4 K at
/// its critical density), DP's search along the isochore takes the EOS's pressure there: DP of Air's state at
/// 13 119 mol/m³ and 132.45 K returns it.
#[cfg(feature = "fluids-all")]
#[test]
fn dp_of_a_pseudo_pure_fluid_where_its_vle_fails() {
    let air = embedded("Air");
    let state = air.state(dt(13_119.0, 132.45)).unwrap();
    let input = Input::dp(Density::molar(13_119.0).unwrap(), Pressure::new(state.p()).unwrap());
    let back = air.state(input);
    assert!(back.as_ref().is_ok_and(|s| (s.t() / 132.45 - 1.0).abs() < 1e-9), "{back:?}");
}
