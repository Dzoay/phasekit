//! Input pairs on a mixture (PLAN.md M13.11): the near-dew HP, PS and PU band of ROT-184's five components, and the
//! mass quality in and out (ROT-201). The capability matrix of every pair (ROT-293, ROT-009) is the conformance kit's,
//! in `tests/mixture_family.rs` (M13.15).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, DataSet, Input, Pair, Registry};
use phasekit_mix::{Equilibrium, Given, Side};
use phasekit_verify::ToleranceClass;

#[path = "common/mix_flash.rs"]
mod mix_flash;
use mix_flash::{binary, mixture};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Parity).unwrap()
}

fn molar(pair: Pair, x: f64, y: f64) -> Input {
    Input::new(pair, x, y, Basis::Molar).unwrap()
}

/// `got` is `want` within class `Flash`.
fn within(got: f64, want: f64) -> bool {
    (got - want).abs() <= ToleranceClass::Flash.bound(want.abs()).unwrap()
}

/// bd:ft05's five components (ROT-183, ROT-184).
const FT05: [(&str, f64); 5] =
    [("Nitrogen", 0.3797), ("Methane", 0.3225), ("Ethane", 0.278), ("n-Butane", 0.0014), ("n-Pentane", 0.0184)];

/// ROT-184 (CoolProp 014c8186's band): ft05 at 0.8 MPa just inside its dew point, T = T_bub + f (T_dew − T_bub) for f
/// from 0.99 to 0.999. The PT flash splits there, 1e-8 < Q < 1 − 1e-8, its phases more than 1e-4 apart in a mole
/// fraction; and HP, PS and PU from its h, s and u return its T, ρ and Q within `Flash`. v8.0.0's GDEM clamp lands such
/// splits on the wrong Rachford–Rice branch, its HSU_P up to 2.97 K off.
#[test]
fn near_dew_hsu_p_band() {
    let fluid = phasekit_mix::mixture(&registry(), &FT05, DataSet::Parity).unwrap();
    let mix = mixture(&fluid);
    let p = 8e5;
    let [bubble, dew] = [Side::Bubble, Side::Dew].map(|side| mix.saturation(side, Given::P(p)).unwrap().t);
    for f in [0.99, 0.995, 0.998, 0.999] {
        let t = bubble + f * (dew - bubble);
        let state = fluid.state(molar(Pair::PT, p, t)).unwrap();
        let q = state.quality().unwrap_or(f64::NAN);
        let apart = match mix.equilibrium(t, p).unwrap() {
            Equilibrium::Split { x, y, .. } => x.iter().zip(&y).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max),
            _ => 0.0,
        };
        assert!(q > 1e-8 && q < 1.0 - 1e-8 && apart > 1e-4, "{f}: Q {q}, {apart}");
        let (h, s, u) = (state.h(Basis::Molar), state.s(Basis::Molar), state.u(Basis::Molar));
        for input in [molar(Pair::HP, h, p), molar(Pair::PS, p, s), molar(Pair::PU, p, u)] {
            let back = fluid.state(input).unwrap();
            let rho = state.rho(Basis::Molar);
            let close = within(back.t(), t)
                && within(back.rho(Basis::Molar), rho)
                && back.quality().is_some_and(|b| (b - q).abs() <= 1e-8);
            assert!(close, "{f} {:?}: {back:?} against T {t}, ρ {rho}, Q {q}", input.pair());
        }
    }
}

/// ROT-201 (map 01 §8; CoolProp-Tests.cpp:4842-4960 on R32/R125 at 50/50): a mass quality reaches the model as one in
/// every quality pair. From QT at 280 K and Q = 0.4, its mass quality Q_m goes back in by mass through QT, PQ, QS, HQ
/// and DQ, each returning the state: T, ρ and Q within `Flash`, Q_m within 1e-10. A molar Q of the same value is another
/// state.
#[test]
fn mass_quality_in_and_out() {
    let fluid = binary(&registry(), "R32&R125", 0.5);
    let state = fluid.state(molar(Pair::QT, 0.4, 280.0)).unwrap();
    let (q, q_mass) = (state.quality().unwrap(), state.mass_quality().unwrap());
    assert!((q_mass - q).abs() > 1e-3, "{q} {q_mass}");
    let (t, p, rho) = (state.t(), state.p(), state.rho(Basis::Molar));
    let (h, s) = (state.h(Basis::Molar), state.s(Basis::Molar));
    let inputs = [
        molar(Pair::QT, q_mass, t),
        molar(Pair::PQ, p, q_mass),
        molar(Pair::QS, q_mass, s),
        molar(Pair::HQ, h, q_mass),
        molar(Pair::DQ, rho, q_mass),
    ];
    for input in inputs {
        let back = fluid.state(input.with_quality_basis(Basis::Mass)).unwrap();
        let close = within(back.t(), t)
            && within(back.rho(Basis::Molar), rho)
            && back.quality().is_some_and(|b| (b - q).abs() <= 1e-8)
            && back.mass_quality().is_some_and(|b| (b - q_mass).abs() <= 1e-10);
        assert!(close, "{:?}: {back:?}", input.pair());
        let molar_q = fluid.state(input).unwrap();
        assert!(molar_q.quality().is_some_and(|b| (b - q_mass).abs() <= 1e-8), "{:?}: {molar_q:?}", input.pair());
    }
}
