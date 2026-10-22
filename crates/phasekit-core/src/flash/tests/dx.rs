//! D+X along the isochore (PLAN.md M7.4).

use super::*;

/// The D+X inputs of `state`, molar: (ρ, h), (ρ, s) and (ρ, u).
#[cfg(feature = "fluids-all")]
fn d_x(state: &State) -> [Input; 3] {
    let (d, m) = (Density::molar(state.rho(Basis::Molar)).unwrap(), Basis::Molar);
    [
        Input::dh(d, Enthalpy::molar(state.h(m)).unwrap()),
        Input::ds(d, Entropy::molar(state.s(m)).unwrap()),
        Input::du(d, InternalEnergy::molar(state.u(m)).unwrap()),
    ]
}

/// D+X under an imposed phase (PLAN.md M7.4, D6): an imposed single phase holds along the whole isochore, an
/// imposed `TwoPhase` at the root. Water: DH, DS and DU of its two-phase state at 400 K (Q 0.5) under `TwoPhase`
/// are that state. Its liquid stretched at 390 K to the saturated liquid's density at 400 K lies in the dome by
/// DT's phase rule, so it is a state only under `Liquid`, under which D+X follow the liquid's isochore and return
/// it. The gas's inputs at 400 K and 10 kPa under `TwoPhase` are refused (a quality above 1).
#[cfg(feature = "fluids-all")]
#[test]
fn d_x_under_a_hint() {
    let water = embedded("Water");
    let (m, with) = (Basis::Molar, |phase| FlashOptions::new().with_phase(phase));
    let two_phase = water.state(q_t(0.5, 400.0)).unwrap();
    let rho_l = water.state(q_t(0.0, 400.0)).unwrap().rho(m);
    assert!(water.state(dt(rho_l, 390.0)).is_ok_and(|s| s.phase() == Phase::TwoPhase), "in the dome");
    let stretched = water.flash(dt(rho_l, 390.0), &with(Phase::Liquid)).unwrap();
    let near = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-9;
    for (truth, phase) in [(&two_phase, Phase::TwoPhase), (&stretched, Phase::Liquid)] {
        for input in d_x(truth) {
            let back = water.flash(input, &with(phase)).unwrap_or_else(|e| panic!("{input:?}: {e:?}"));
            let same = back.phase() == phase && near(back.t(), truth.t()) && near(back.rho(m), truth.rho(m));
            let quality = match (back.quality(), truth.quality()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-12,
                (a, b) => a == b,
            };
            assert!(same && quality, "{input:?}: {back:?}");
        }
    }
    for input in d_x(&water.state(pt(1e4, 400.0)).unwrap()) {
        let refused = water.flash(input, &with(Phase::TwoPhase));
        assert!(
            matches!(refused, Err(Error::InvalidInput { quantity: Quantity::QUALITY, .. })),
            "{input:?}: {refused:?}"
        );
    }
}
