//! A mixture at a fixed composition (PLAN.md M13.4): composition rules, the pure-fluid limit, the ideal part's
//! contract, limits and exact virials.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, DataSet, Density, Derivs, Error, FlashOptions, Fluid, Input, Limit, Order, Pair, Phase, Pressure, Quantity,
    Registry, Temperature, math,
};
use phasekit_mix::{R_CODATA, mixture};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Corrected).unwrap()
}

fn dt(rho: f64, t: f64) -> Input {
    Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())
}

/// The compressed liquid at 5 MPa and `t`: its DT input, from a PT solve on the liquid branch.
fn liquid(fluid: &Fluid, t: f64) -> Input {
    let pt = Input::pt(Pressure::new(5e6).unwrap(), Temperature::new(t).unwrap());
    dt(fluid.flash(pt, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap().rho(Basis::Molar), t)
}

/// A state's p, h, s, c_v, c_p and w, bits and all.
fn bits(fluid: &Fluid, input: Input, phase: Phase) -> [u64; 6] {
    let s = fluid.flash(input, &FlashOptions::new().with_phase(phase)).unwrap();
    [
        s.p(),
        s.h(Basis::Molar),
        s.s(Basis::Molar),
        s.cv(Basis::Molar).unwrap(),
        s.cp(Basis::Molar).unwrap(),
        s.speed_of_sound().unwrap(),
    ]
    .map(f64::to_bits)
}

/// ROT-099, ROT-208: a composition that trims to one component is that pure fluid itself, the registry's own handle,
/// so it equals the pure fluid bit for bit under any gauge; v8.0.0 keeps R_CODATA there (p −1.008e-6 for R32).
#[test]
fn unit_fraction_is_the_pure_fluid() {
    let registry = registry();
    let r32 = registry.get("R32").unwrap();
    let alone = mixture(&registry, &[("R32", 1.0), ("R125", 0.0)], DataSet::Corrected).unwrap();
    assert_eq!(alone.info().key(), r32.info().key());
    let at = liquid(r32, 280.0);
    assert_eq!(bits(&alone, at, Phase::Liquid), bits(r32, at, Phase::Liquid));
    let gauged = registry.with_reference("R32", phasekit_core::ReferenceState::Iir).unwrap();
    let alone = mixture(&gauged, &[("R32", 1.0), ("R125", 0.0)], DataSet::Corrected).unwrap();
    assert_eq!(bits(&alone, at, Phase::Liquid), bits(gauged.get("R32").unwrap(), at, Phase::Liquid));
}

/// Near a unit fraction the mixture approaches the pure fluid up to user decision MX3's R ratio: at x₂ = 1e-9, p is the
/// pure fluid's times R_CODATA/R_R32 in its ideal part (the residual is weighted by the mixture's R), and h, c_p within
/// that ratio's size of it.
#[test]
fn near_unit_fraction_approaches_the_pure_fluid() {
    let registry = registry();
    let r32 = registry.get("R32").unwrap();
    let near = mixture(&registry, &[("R32", 1.0 - 1e-9), ("R125", 1e-9)], DataSet::Corrected).unwrap();
    let r_r32 = r32.model().helmholtz().unwrap().gas_constant();
    for (input, phase) in [(liquid(r32, 280.0), Phase::Liquid), (dt(100.0, 320.0), Phase::Gas)] {
        let t = input.values().1;
        let (a, b) = (
            near.flash(input, &FlashOptions::new().with_phase(phase)).unwrap(),
            r32.flash(input, &FlashOptions::new().with_phase(phase)).unwrap(),
        );
        let ratio = (R_CODATA / r_r32 - 1.0).abs();
        for (what, x, y) in [
            ("p", a.p(), b.p()),
            ("h", a.h(Basis::Molar), b.h(Basis::Molar)),
            ("cp", a.cp(Basis::Molar).unwrap(), b.cp(Basis::Molar).unwrap()),
        ] {
            assert!(((x - y) / y).abs() <= 2.0 * ratio + 1e-8, "{what} at {t} K: {x} against {y}");
        }
    }
}

/// ROT-193: a composition that is not one is refused: fractions summing to 0.6, a negative one, a NaN, more than 1.
#[test]
fn composition_is_validated() {
    let registry = registry();
    for z in [[0.3, 0.3], [-0.1, 1.1], [f64::NAN, 0.5], [1.5, -0.5]] {
        let refused = mixture(&registry, &[("R32", z[0]), ("R125", z[1])], DataSet::Corrected);
        assert!(matches!(refused, Err(Error::InvalidInput { .. })), "{z:?}: {refused:?}");
    }
}

/// ROT-192: a component at x = 0 is trimmed, so the mixture is the one of the others, bit for bit; v8.0.0 keeps it and
/// its ln x term (0/0 with two zeros).
#[test]
fn zero_components_equal_the_trimmed_mixture() {
    let registry = registry();
    let three = mixture(&registry, &[("R32", 0.6), ("R125", 0.4), ("R134a", 0.0)], DataSet::Corrected).unwrap();
    let two = mixture(&registry, &[("R32", 0.6), ("R125", 0.4)], DataSet::Corrected).unwrap();
    assert_eq!(three.info().key(), two.info().key());
    let at = liquid(&two, 290.0);
    assert_eq!(bits(&three, at, Phase::Liquid), bits(&two, at, Phase::Liquid));
}

/// ROT-197: a mixture's limits are its components' intersection (v8.0.0 averages them by mole fraction): R32's
/// T_min and T_max are 136.34 K and 435 K, R125's 172.52 K and 500 K.
#[test]
fn limits_are_the_components_intersection() {
    let registry = registry();
    let fluids = ["R32", "R125"].map(|n| registry.get(n).unwrap().model().limits().copied().unwrap());
    let mix = mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Corrected).unwrap();
    let limits = mix.model().limits().copied().unwrap();
    assert_eq!(limits.t_min(), fluids[0].t_min().max(fluids[1].t_min()));
    assert_eq!(limits.t_max(), fluids[0].t_max().min(fluids[1].t_max()));
    assert_eq!(limits.p_max(), fluids[0].p_max().min(fluids[1].p_max()));
}

/// The ideal part keeps core's contract, its δ-part ln δ exactly (A01 = 1, A02 = −1, ...), so p, the density solve and
/// the acceptance gate agree; its τ-part is the components' weighted by z_i R_i/R, so the ideal-gas h and c_p are the
/// z-weighted sum of theirs, each with its own R (MX3, as v8.0.0).
#[test]
fn the_ideal_part_keeps_the_delta_contract_and_each_component_s_r() {
    let registry = registry();
    let mix = mixture(&registry, &[("R32", 0.3), ("R125", 0.7)], DataSet::Corrected).unwrap();
    let ideal = mix.model().helmholtz().unwrap().ideal(300.0, 2_000.0, Order::Four);
    for j in 1..=4 {
        assert_eq!(ideal.get(0, j), Derivs::IDEAL_DELTA.get(0, j), "A0{j}");
    }
    // h⁰ = RT(1 + A10): the mixture's is Σ z_i h⁰_i but for the ideal δ-part's RT, which is R's where theirs is R_i's.
    let (r, h0) = (
        |f: &Fluid| f.model().helmholtz().unwrap().gas_constant(),
        |f: &Fluid| {
            let eos = f.model().helmholtz().unwrap();
            eos.gas_constant() * 300.0 * (1.0 + eos.ideal(300.0, 2_000.0, Order::One).get(1, 0).unwrap())
        },
    );
    let (r32, r125) = (registry.get("R32").unwrap(), registry.get("R125").unwrap());
    let want = 0.3 * h0(r32) + 0.7 * h0(r125) + 300.0 * (R_CODATA - 0.3 * r(r32) - 0.7 * r(r125));
    assert!((h0(&mix) - want).abs() <= 1e-12 * want.abs(), "{} against {want}", h0(&mix));
}

/// E4: the virials are exact Taylor coefficients: B = lim α^r_δ/ρ_r and C = lim α^r_δδ/ρ_r² as δ → 0, here against
/// the residual's own derivatives at a very small density, and dB/dT and dC/dT against central differences; with and
/// without departure functions (Methane/Ethane, R32/R125, Methane/n-Hexane), and with a departure function weighted by
/// F ≠ 1 (Methane/IsoButane, GERG-2008's generalized one at F = 0.771).
#[test]
fn virials_are_exact_taylor_coefficients() {
    let registry = registry();
    for names in [["Methane", "Ethane"], ["R32", "R125"], ["Methane", "n-Hexane"], ["Methane", "IsoButane"]] {
        let mix = mixture(&registry, &[(names[0], 0.35), (names[1], 0.65)], DataSet::Corrected).unwrap();
        let eos = mix.model().helmholtz().unwrap();
        let t = 400.0;
        let v = eos.zero_density(t).unwrap();
        let rho = 1e-4;
        let d = eos.residual(t, rho, Order::Two);
        let (b, c) = (d.get(0, 1).unwrap() / rho, d.get(0, 2).unwrap() / (rho * rho));
        assert!(((b - v.b) / v.b).abs() < 1e-6, "{names:?} B {b} against {}", v.b);
        assert!(((c - v.c) / v.c).abs() < 1e-3, "{names:?} C {c} against {}", v.c);
        let h = 1e-4 * t;
        let (lo, hi) = (eos.zero_density(t - h).unwrap(), eos.zero_density(t + h).unwrap());
        let fd = (hi.b - lo.b) / (2.0 * h);
        assert!((fd / v.db_dt - 1.0).abs() < 1e-7, "{names:?} dB/dT {fd} against {}", v.db_dt);
        assert!((((hi.c - lo.c) / (2.0 * h)) / v.dc_dt - 1.0).abs() < 1e-6, "{names:?} dC/dT");
        assert!(math::ln(rho) < 0.0);
    }
}

/// A mixture keeps the composition it was given, its zero fractions trimmed (ROT-192).
#[test]
fn a_mixture_keeps_its_composition() {
    let registry = registry();
    let fluid = mixture(&registry, &[("R32", 0.3), ("R134a", 0.0), ("R125", 0.7)], DataSet::Corrected).unwrap();
    let mix = (fluid.model() as &dyn std::any::Any).downcast_ref::<phasekit_mix::Mixture>().unwrap();
    assert_eq!(mix.composition(), &[0.3, 0.7]);
    assert_eq!(mix.model().len(), 2);
}

/// ROT-197: under the default `DomainPolicy::Enforce` a state above the components' T_max is refused, as a pure
/// fluid's is; under `Extrapolate` it is evaluated and flagged.
#[test]
fn states_outside_the_limits_are_refused_unless_extrapolated() {
    let registry = registry();
    let mix = mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Corrected).unwrap();
    let hot = dt(100.0, 450.0); // R32's T_max is 435 K
    let refused = mix.flash(hot, &FlashOptions::new().with_phase(Phase::Gas));
    assert!(matches!(refused, Err(Error::Domain(_))), "{refused:?}");
    let opts = FlashOptions::new().with_phase(Phase::Gas).with_domain(phasekit_core::DomainPolicy::Extrapolate);
    assert!(mix.flash(hot, &opts).unwrap().is_extrapolated());
    assert!(!mix.flash(dt(100.0, 320.0), &opts).unwrap().is_extrapolated());
}

/// A value no state of the feed reaches names the extreme its search reached (PLAN.md M29.2; user decision EM16): an h
/// at 300 K above and below every state's and an h at Q = 0.5 below every saturated state's, each as the caller gave it, mass
/// basis; the bound is reached, moved inside by 1e-9 of it, under `Extrapolate`, and 1e-6 past it is not. The lowest h
/// at 300 K is the densest end's, whose homogeneous root the unhinted flash does not keep, and a mixture's HT takes no
/// hint, so that bound is checked from past it only.
#[test]
fn an_unreachable_value_names_its_bound() {
    let registry = registry();
    let mix = mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Corrected).unwrap();
    let extrapolate = FlashOptions::new().with_domain(phasekit_core::DomainPolicy::Extrapolate);
    type Make = fn(f64) -> Result<Input, Error>;
    /// The input, the value, the limit, the given input, and whether the flash reaches the bound.
    type Case = (Make, f64, Limit, (&'static Quantity, f64), bool);
    let cases: [Case; 3] = [
        (|h| Input::new(Pair::HT, h, 300.0, Basis::Mass), 1e7, Limit::Max, (Quantity::TEMPERATURE, 300.0), true),
        (|h| Input::new(Pair::HT, h, 300.0, Basis::Mass), -1e7, Limit::Min, (Quantity::TEMPERATURE, 300.0), false),
        (|h| Input::new(Pair::HQ, h, 0.5, Basis::Mass), -1e7, Limit::Min, (Quantity::QUALITY, 0.5), true),
    ];
    for (make, value, limit, given, reach) in cases {
        let refused = mix.state(make(value).unwrap());
        let Err(Error::Unreachable { quantity, value: v, limit: l, bound, given: g }) = refused else {
            panic!("{value}: {refused:?}")
        };
        assert_eq!((quantity, v, l, g), (Quantity::enthalpy(Basis::Mass), value, limit, given), "{value}");
        let inward = if limit == Limit::Max { -bound.abs() } else { bound.abs() };
        let reached = mix.flash(make(bound + 1e-9 * inward).unwrap(), &extrapolate);
        assert!(
            !reach || matches!(reached, Ok(_) | Err(Error::Ambiguous { .. })),
            "{value} inside {bound}: {reached:?}"
        );
        let beyond = mix.flash(make(bound - 1e-6 * inward).unwrap(), &extrapolate);
        assert!(
            matches!(beyond, Err(Error::Unreachable { limit: l, .. }) if l == limit),
            "{value} past {bound}: {beyond:?}"
        );
    }
}

/// With a phase hint, PT finds the hinted branch's density and gives back the state DT gives there; without one, DT
/// tests the state's stability first (M13.10) and returns that same compressed liquid, labelled from the critical point
/// (5 MPa lies above it).
#[test]
fn hinted_flashes_reach_single_phase_states() {
    let registry = registry();
    let mix = mixture(&registry, &[("R32", 0.5), ("R125", 0.5)], DataSet::Corrected).unwrap();
    let at = liquid(&mix, 280.0);
    let state = mix.flash(at, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap();
    let unhinted = mix.flash(at, &FlashOptions::new()).unwrap();
    assert_eq!((unhinted.p(), unhinted.phase()), (state.p(), Phase::SupercriticalLiquid));
    assert!(((state.p() - 5e6) / 5e6).abs() < 1e-12, "{}", state.p());
    let gas = Input::pt(Pressure::new(1e5).unwrap(), Temperature::new(280.0).unwrap());
    let vapour = mix.flash(gas, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
    assert!(vapour.rho(Basis::Molar) < 100.0 && ((vapour.p() - 1e5) / 1e5).abs() < 1e-12, "{:?}", vapour);
}
