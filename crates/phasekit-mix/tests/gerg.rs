//! GERG-2008 (PLAN.md M13.5): its names, its pure fluids and their limits, its reference state and its refusals; its
//! check values are `phasekit-verify`'s `tests/gerg.rs`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, DataSet, Density, DomainPolicy, Error, FlashOptions, Fluid, Input, Order, Pair, Phase, Registry, Strategy,
    Temperature, math,
};
use phasekit_mix::{GERG_PREFIX, MultiFluid, R_GERG, gerg2008, with_gerg2008};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Corrected).unwrap()
}

/// GERG-2008's 21 components, as the registry names them, in AGA8's order.
const GERG: [&str; 21] = [
    "Methane",
    "Nitrogen",
    "CarbonDioxide",
    "Ethane",
    "n-Propane",
    "IsoButane",
    "n-Butane",
    "Isopentane",
    "n-Pentane",
    "n-Hexane",
    "n-Heptane",
    "n-Octane",
    "n-Nonane",
    "n-Decane",
    "Hydrogen",
    "Oxygen",
    "CarbonMonoxide",
    "Water",
    "HydrogenSulfide",
    "Helium",
    "Argon",
];

/// A state's p, h, s, c_v, c_p and w, bits and all.
fn bits(fluid: &Fluid, t: f64, rho: f64) -> [u64; 6] {
    let opts = FlashOptions::new().with_phase(Phase::Gas).with_domain(DomainPolicy::Extrapolate);
    let s = fluid.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &opts).unwrap();
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

/// User decision MX6: `with_gerg2008` registers `GERG2008::` and the registry's name of each of the 21 components,
/// with its aliases so prefixed (`GERG2008::CH4`), R = 8.314472 (MX3), GERG's 60-700 K and 70 MPa as
/// its limits and the component's own triple point (ROT-205); GERG's critical density and temperature reduce it.
#[test]
fn with_gerg2008_registers_the_21_pure_components() {
    let base = registry();
    let registry = with_gerg2008(&base).unwrap();
    for name in GERG {
        let gerg = registry.get(&format!("{GERG_PREFIX}{name}")).unwrap();
        let reference = base.get(name).unwrap();
        assert_eq!(gerg.info().name(), format!("GERG2008::{}", reference.info().name()));
        assert_eq!(gerg.model().helmholtz().unwrap().gas_constant(), R_GERG);
        let limits = gerg.model().limits().unwrap();
        assert_eq!((limits.t_min(), limits.t_max(), limits.p_max()), (60.0, 700.0, 70e6), "{name}");
        assert_eq!(limits.t_triple(), reference.model().limits().unwrap().t_triple(), "{name}");
        assert_eq!(&*gerg.info().source().bibkey, "Kunz-JCED-2012");
    }
    assert_eq!(registry.get("GERG2008::CH4").unwrap().info().name(), "GERG2008::Methane");
    let methane = registry.get("GERG2008::Methane").unwrap().model().helmholtz().unwrap().reducing().unwrap();
    assert_eq!((methane.t, methane.rho), (190.564, 1e3 * 10.139342719));
}

/// A composition that trims to one component is GERG's pure fluid, bit for bit the registered one; a mixture's name
/// carries the prefix once (MX6).
#[test]
fn one_component_is_the_gerg_pure_fluid() {
    let base = registry();
    let registered = with_gerg2008(&base).unwrap();
    let pure = registered.get("GERG2008::Methane").unwrap();
    let trimmed = gerg2008(&base, &[("Methane", 1.0), ("Ethane", 0.0)]).unwrap();
    assert_eq!(trimmed.info().key(), pure.info().key());
    assert_eq!(bits(&trimmed, 250.0, 3_000.0), bits(pure, 250.0, 3_000.0));
    let mix = gerg2008(&base, &[("Methane", 0.9), ("ETHANE", 0.1)]).unwrap();
    assert_eq!(mix.info().name(), "GERG2008::Methane[0.9]&Ethane[0.1]");
    assert_eq!(mix.model().helmholtz().unwrap().gas_constant(), R_GERG);
}

/// A component outside GERG-2008 is `UnknownName("GERG2008::<name>")`; one component twice, or fewer than two in a
/// model, is refused; a composition is validated as any mixture's (ROT-193).
#[test]
fn gerg2008_refuses_what_it_is_not() {
    let registry = registry();
    let refused = gerg2008(&registry, &[("Methane", 0.5), ("R32", 0.5)]).unwrap_err();
    assert!(refused.to_string().contains("GERG2008::R32"), "{refused}");
    for names in [&["Methane"][..], &["Methane", "CH4"]] {
        let error = MultiFluid::gerg2008(&registry, names).unwrap_err();
        assert!(matches!(error, Error::InvalidInput { .. }), "{names:?}: {error}");
    }
    let unnormalised = gerg2008(&registry, &[("Methane", 0.5), ("Ethane", 0.4)]);
    assert!(matches!(unnormalised, Err(Error::InvalidInput { .. })));
}

/// ROT-195: of GERG-2008's 210 binaries, 15 carry a departure function (seven binary-specific, eight the generalized
/// one), and only those are evaluated: all 15 among the eight components they join, none among the rest.
#[test]
fn fifteen_binaries_carry_a_departure_function() {
    let registry = registry();
    let all = MultiFluid::gerg2008(&registry, &GERG).unwrap();
    assert_eq!((all.len(), all.departures()), (21, 15));
    let joined = ["Methane", "Nitrogen", "CarbonDioxide", "Ethane", "n-Propane", "IsoButane", "n-Butane", "Hydrogen"];
    assert_eq!(MultiFluid::gerg2008(&registry, &joined).unwrap().departures(), 15);
    let rest: Vec<&str> = GERG.iter().copied().filter(|n| !joined.contains(n)).collect();
    assert_eq!(MultiFluid::gerg2008(&registry, &rest).unwrap().departures(), 0);
}

/// GERG-2008's ideal gas has h = s = 0 at 298.15 K and 101.325 kPa for every component (Kunz and Wagner 2012), which
/// fixes its n₁ and n₂ (to 1e-12 of RT and R: n-decane's terms reach about 100, and cancel): AGA8 prints them to ten digits, phasekit computes them, and each printed one is the computed
/// one rounded to its last printed digit.
#[test]
fn the_ideal_gas_reference_state_is_exact() {
    let registry = with_gerg2008(&registry()).unwrap();
    let (t0, rho0) = (298.15, 101_325.0 / (R_GERG * 298.15));
    for name in GERG {
        let eos = registry.get(&format!("{GERG_PREFIX}{name}")).unwrap().model().helmholtz().unwrap();
        let ideal = eos.ideal(t0, rho0, Order::One);
        let (a00, a10) = (ideal.get(0, 0).unwrap(), ideal.get(1, 0).unwrap());
        // h⁰ = RT(1 + A10), s⁰ = R(A10 − A00) with the ideal gas's A01 = 1.
        assert!((1.0 + a10).abs() < 1e-12, "{name}: h0/RT = {}", 1.0 + a10);
        assert!((a10 - a00).abs() < 1e-12, "{name}: s0/R = {}", a10 - a00);
    }
    let entries = phasekit_mix::internal::gerg_2008_aga8();
    for (i, computed) in phasekit_mix::internal::gerg_2008_n1_n2().iter().enumerate() {
        for (j, value) in computed.iter().enumerate() {
            let printed = entries.iter().find(|(n, idx, _)| *n == "n0i" && idx[..] == [i + 1, j + 1]).unwrap().2;
            let decimals = printed.to_string().split_once('.').map_or(0, |(_, f)| f.len());
            let half = 0.5 * math::powi(10.0, -(decimals as i32));
            assert!(
                (printed - value).abs() <= half * (1.0 + 1e-9),
                "{} n{}: {printed} against {value}",
                GERG[i],
                j + 1
            );
        }
    }
}

/// E4: a GERG pure fluid's virials are exact Taylor coefficients: B = lim α^r_δ/ρ_c and C = lim α^r_δδ/ρ_c², here
/// against the residual's own derivatives at a very small density, and dB/dT and dC/dT against central differences.
#[test]
fn virials_are_exact_taylor_coefficients() {
    let registry = with_gerg2008(&registry()).unwrap();
    for name in ["GERG2008::Methane", "GERG2008::Water", "GERG2008::Helium"] {
        let eos = registry.get(name).unwrap().model().helmholtz().unwrap();
        let (t, rho) = (400.0, 1e-4);
        let v = eos.zero_density(t).unwrap();
        let d = eos.residual(t, rho, Order::Two);
        let (b, c) = (d.get(0, 1).unwrap() / rho, d.get(0, 2).unwrap() / (rho * rho));
        assert!(((b - v.b) / v.b).abs() < 1e-6, "{name} B {b} against {}", v.b);
        assert!(((c - v.c) / v.c).abs() < 1e-3, "{name} C {c} against {}", v.c);
        let h = 1e-4 * t;
        let (up, down) = (eos.zero_density(t + h).unwrap(), eos.zero_density(t - h).unwrap());
        let fd = (up.b - down.b) / (2.0 * h);
        assert!((fd / v.db_dt - 1.0).abs() < 1e-7, "{name} dB/dT {fd} against {}", v.db_dt);
        let fd = (up.c - down.c) / (2.0 * h);
        assert!((fd / v.dc_dt - 1.0).abs() < 1e-6, "{name} dC/dT {fd} against {}", v.dc_dt);
    }
}

/// A GERG pure fluid's published critical point is GERG's critical temperature and density (its reducing state) with
/// its own equation's pressure there, its molar mass AGA8's, and its saturation the registry curve's range below that
/// critical temperature (as a seed for core's VLE).
#[test]
fn pure_fluids_carry_gerg_s_critical_point_and_mass() {
    let base = registry();
    let registry = with_gerg2008(&base).unwrap();
    let entries = phasekit_mix::internal::gerg_2008_aga8();
    let get = |name: &str, i: usize| entries.iter().find(|(n, idx, _)| *n == name && idx[..] == [i]).unwrap().2;
    for (i, name) in GERG.iter().enumerate().map(|(i, n)| (i + 1, n)) {
        let fluid = registry.get(&format!("{GERG_PREFIX}{name}")).unwrap();
        let c = fluid.info().published_critical().unwrap();
        assert_eq!((c.t, c.rho), (get("Tc", i), 1e3 * get("Dc", i)), "{name}");
        let at = Input::new(Pair::DT, c.rho, c.t, Basis::Molar).unwrap();
        let options = FlashOptions::new().with_phase(Phase::Supercritical).with_domain(DomainPolicy::Extrapolate);
        assert_eq!(c.p, fluid.flash(at, &options).unwrap().p(), "{name}");
        assert_eq!(fluid.info().molar_mass(), 1e-3 * get("MMiGERG", i), "{name}");
        let package = (fluid.model() as &dyn std::any::Any).downcast_ref::<phasekit_core::PureFluid>().unwrap();
        let reference =
            (base.get(name).unwrap().model() as &dyn std::any::Any).downcast_ref::<phasekit_core::PureFluid>();
        let reference = reference.unwrap().saturation().unwrap().unwrap().t_range();
        let range = package.saturation().unwrap().unwrap().t_range();
        assert_eq!(range, (reference.0, reference.1.min(c.t)), "{name}");
    }
}

/// A GERG-2008 model does not depend on the order its components are named in: isobutane/n-butane, whose β AGA8
/// stores for its own order, gives the same reducing state either way round, and the same residual.
#[test]
fn swapping_components_changes_nothing() {
    let registry = registry();
    let ab = MultiFluid::gerg2008(&registry, &["IsoButane", "n-Butane"]).unwrap();
    let ba = MultiFluid::gerg2008(&registry, &["n-Butane", "IsoButane"]).unwrap();
    let ((t1, r1), (t2, r2)) = (ab.reducing_state(&[0.3, 0.7]), ba.reducing_state(&[0.7, 0.3]));
    assert!(((t1 - t2) / t1).abs() <= 1e-15 && ((r1 - r2) / r1).abs() <= 1e-15, "{t1} {r1} against {t2} {r2}");
    let (d1, d2) = (
        ab.residual(&[0.3, 0.7], 300.0, 8e3, Order::Two).unwrap(),
        ba.residual(&[0.7, 0.3], 300.0, 8e3, Order::Two).unwrap(),
    );
    assert!((d1.get(0, 1).unwrap() - d2.get(0, 1).unwrap()).abs() <= 1e-13);
}

/// ROT-083: a GERG pure fluid's HP, PS and PU flashes take their brackets from its own model, not from fluid metadata
/// (CoolProp's GERG backend, after v8.0.0, fails them for want of a triple-point pressure): from a compressed liquid, a
/// supercritical state and a saturated mixture (core's generic VLE, as no saturation curve is given), each pair gives
/// back T and ρ within `Flash`'s 1e-9, and Q within its 1e-8.
#[test]
fn pure_hp_ps_pu_round_trip() {
    let registry = with_gerg2008(&registry()).unwrap();
    let cases = [
        ("GERG2008::Methane", [(120.0, 5e6), (300.0, 5e6)], 150.0),
        ("GERG2008::n-Propane", [(250.0, 2e6), (450.0, 6e6)], 300.0),
        ("GERG2008::Water", [(350.0, 1e6), (700.0, 5e6)], 450.0),
    ];
    for (name, single, saturated) in cases {
        let fluid = registry.get(name).unwrap();
        let mut truths: Vec<_> = single
            .iter()
            .map(|&(t, p)| {
                fluid.flash(Input::new(Pair::PT, p, t, Basis::Molar).unwrap(), &FlashOptions::new()).unwrap()
            })
            .collect();
        let qt = Input::new(Pair::QT, 0.5, saturated, Basis::Molar).unwrap();
        let split = fluid.flash(qt, &FlashOptions::new()).unwrap();
        // GERG's own saturation: the registry curve's seeds, polished by core's VLE on GERG's equation.
        assert_eq!(split.path().strategy, Strategy::Vle, "{name}");
        truths.push(split);
        for truth in &truths {
            let inputs = [
                (Pair::HP, truth.h(Basis::Molar), truth.p()),
                (Pair::PS, truth.p(), truth.s(Basis::Molar)),
                (Pair::PU, truth.p(), truth.u(Basis::Molar)),
            ];
            for (pair, x, y) in inputs {
                let back = fluid.flash(Input::new(pair, x, y, Basis::Molar).unwrap(), &FlashOptions::new());
                let back = back.unwrap_or_else(|e| panic!("{name} {pair:?} at {} K: {e}", truth.t()));
                let close = |a: f64, b: f64| ((a - b) / b).abs() <= 1e-9;
                assert!(close(back.t(), truth.t()), "{name} {pair:?}: T {} against {}", back.t(), truth.t());
                let (rho, want) = (back.rho(Basis::Molar), truth.rho(Basis::Molar));
                assert!(close(rho, want), "{name} {pair:?} at {} K: ρ {rho} against {want}", truth.t());
                let (q, want) = (back.quality().unwrap_or(-1.0), truth.quality().unwrap_or(-1.0));
                assert!((q - want).abs() <= 1e-8, "{name} {pair:?}: Q {q} against {want}");
            }
        }
    }
}
