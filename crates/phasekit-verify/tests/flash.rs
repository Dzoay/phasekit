//! L4 flash (VERIFICATION.md §3.5 `flash`, §8.2): pairs read off truth states, never density bands (map 12 §6.4),
//! against CoolProp's flash of the same inputs. Since M5.3 the DT pair, below the critical temperature included; since
//! M7.1 PT.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, DataSet, Density, DomainError, Error, Input, Pair, Phase, Pressure, Prop, Quality, Registry, Temperature,
    math,
};
use phasekit_verify::flash::FlashCheck;
use phasekit_verify::{Fixture, fixture};

/// The core subset's `flash` files (VERIFICATION.md §3.6).
const FLASH_CORE: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/flash/Air.csv"),
    fixture!("coolprop-8.0.0/flash/Ammonia.csv"),
    fixture!("coolprop-8.0.0/flash/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/flash/HFE143m.csv"),
    fixture!("coolprop-8.0.0/flash/Helium.csv"),
    fixture!("coolprop-8.0.0/flash/Methanol.csv"),
    fixture!("coolprop-8.0.0/flash/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/flash/R1130(E).csv"),
    fixture!("coolprop-8.0.0/flash/R1234yf.csv"),
    fixture!("coolprop-8.0.0/flash/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/flash/R125.csv"),
    fixture!("coolprop-8.0.0/flash/R410A.csv"),
    fixture!("coolprop-8.0.0/flash/Water.csv"),
    fixture!("coolprop-8.0.0/flash/n-Heptane.csv"),
];

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/flash/<Fluid>.csv, the DT rows (PLAN.md M5.3): (ρ, T) read off 6 × 6
/// (p, T) and 4 × 4 (T, Q) truth states per core fluid, flashed with no phase imposed. The phase label, Q (class
/// `Flash`, 1e-8 absolute) and p, h, s, u (class `Flash`, 1e-9 of max(|v|, floor); for a single-phase state with `Term`
/// carried through the relation, user decision TC1) match; inside the dome the state is the lever rule on the
/// superancillary's densities (`State::from_split`). CoolProp has no QT at 0 < Q < 1 for the pseudo-pure Air and R410A,
/// so their (T, Q) cells are `dome` truths, ρ between their QT densities (M6.9): inside their dome the state is the
/// pure VLE of their EOS, as CoolProp's.
#[test]
fn dt_two_phase_matches_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        check.dt_rows(&fixture, registry.get(name).unwrap(), &record);
    }
    assert_eq!(check.report(20), None);
    // 14 × 36 PT truths and 14 × 16 QT truths, the pseudo-pure Air's and R410A's `dome` truths (M6.9), of which two
    // of Air's are gas.
    assert_eq!((check.two_phase, check.compared), (12 * 16 + 30, 14 * 36 + 14 * 16));
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/flash/<Fluid>.csv, the PT rows (PLAN.md M7.1): each core fluid's 6 × 6
/// (p, T) truths, CoolProp's PT flash of its own inputs. Phase and ρ match, ρ within `Flash`. The oracle's p, h, s and u
/// are exempt (DIV-0020): it reports them at its solver's last iterate, not at its ρ. phasekit's p is the input
/// (`pt_round_trips_on_a_40x40_log_p_t_grid`) and its h, s and u are its DT's at that ρ, which the DT row of the same
/// truth checks against the oracle's DT (`dt_two_phase_matches_oracle`).
#[test]
fn pt_matches_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        check.pt_rows(&fixture, registry.get(name).unwrap(), &record);
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.two_phase, check.compared, check.exempt), (0, 14 * 36, 4 * 14 * 36));
}

/// PLAN.md M7.1 (ROT-074's round trips), class `Flash`: PT of each core fluid on a 40 × 40 grid, the cell centres of T
/// in [max(Tmin, Ttriple), Tmax] and of log p in [1 Pa, pmax], round-trips (`FlashCheck::pt_round_trip`): DT at its
/// (ρ, T) gives the same phase, and its p is the input's within `Flash`. Every point flashes but two, refused by typed
/// errors where CoolProp refuses too: Air at 84.0 K and 171 kPa lies between its dew and bubble pressure ancillaries,
/// two-phase, where (p, T) fix no quality; Methanol at 181.2 K and 619 MPa, deep below its melting line (236 K there),
/// has cv < 0 on its EOS (above 500 MPa at that T), which the acceptance gate refuses.
#[test]
fn pt_round_trips_on_a_40x40_log_p_t_grid() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    let mut refused = Vec::new();
    let centre = |k: usize| (k as f64 + 0.5) / 40.0;
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let limits = record.limits;
        let t_low = limits.t_triple().map_or(limits.t_min(), |t| t.max(limits.t_min()));
        for i in 0..40 {
            let t = t_low + (limits.t_max() - t_low) * centre(i);
            for j in 0..40 {
                let p = math::exp(math::ln(limits.p_max()) * centre(j));
                if let Err(e) = check.pt_round_trip(name, (registry.get(name).unwrap(), &record), (p, t)) {
                    refused.push((name, (p.round(), (t * 1e3).round() / 1e3), e));
                }
            }
        }
    }
    assert_eq!(check.report(20), None);
    let two_phase = Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase };
    let unstable = Error::Domain(DomainError::MechanicallyUnstable);
    assert_eq!(refused, [("Air", (170_541.0, 84.003), two_phase), ("Methanol", (619_157_820.0, 181.165), unstable)]);
    assert_eq!(check.compared, 14 * 1600 - 2);
}

/// PLAN.md M7.3 (VERIFICATION.md §8.2), class `Flash`: PH, PS and PU of every truth in the core subset's `flash` files,
/// 6 × 6 (p, T) truths and 4 × 4 (T, Q) truths per fluid (the pseudo-pure Air's and R410A's (p, Q) `dome` truths, all
/// but two of Air's, gas by CoolProp's bands and two-phase by its PH), return the truth's phase, T and ρ within 1e-9
/// and, in the dome, its quality within 1e-8 (`FlashCheck::px_round_trips`).
#[test]
fn p_x_round_trips() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        check.px_round_trips(&fixture, registry.get(name).unwrap());
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.two_phase, check.compared), (3 * (14 * 16 - 2), 3 * (14 * (36 + 16) - 2)));
}

/// PLAN.md M7.4 (VERIFICATION.md §8.2), class `Flash`: DH, DS and DU of every truth in the core subset's `flash` files,
/// 6 × 6 (p, T) and 4 × 4 (T, Q) truths per fluid and the pseudo-pure fluids' `dome` truths as their DT states (two of
/// Air's gas), return the truth's phase, T and ρ within 1e-9 and, in the dome, its quality within 1e-8
/// (`FlashCheck::dx_round_trips`). And map 03 §8 AirCritical (`CoolProp-Tests-AirCritical.cpp:33`): Air, pseudo-pure,
/// on CoolProp's 60 × 40 grid of T/Tc in [0.96, 2] and p/pc in [0.5, 5], every single-phase PT state round-trips the
/// same way, where CoolProp asks 2e-4 (its D+X threw "p is not a valid number" near ρc before that test). On the way
/// the search passes where Air's VLE fails, just below its published Tc (`flash::isochore_x`).
#[test]
fn d_x_round_trips() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        check.dx_round_trips(&fixture, registry.get(name).unwrap());
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.two_phase, check.compared), (3 * (14 * 16 - 2), 3 * 14 * (36 + 16)));

    let mut air = FlashCheck::default();
    let fluid = registry.get("Air").unwrap();
    let c = phasekit_core::internal::record(&registry, "Air").unwrap().critical.unwrap();
    let mut refused = Vec::new();
    for i in 0..60 {
        let t = (0.96 + (2.0 - 0.96) * f64::from(i) / 59.0) * c.t;
        for j in 0..40 {
            let p = (0.5 + (5.0 - 0.5) * f64::from(j) / 39.0) * c.p;
            match fluid.state(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())) {
                Ok(truth) => air.dx_round_trip("Air", fluid, &truth),
                Err(e) => refused.push((i, j, e)),
            }
        }
    }
    assert_eq!(air.report(20), None);
    // As CoolProp's test skips its PT throws: one point lies between Air's pressure ancillaries, 131.90 K at 3.64 MPa.
    assert_eq!(refused, [(2, 4, Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase })]);
    assert_eq!(air.compared, 3 * (2400 - 1));
}

/// PLAN.md M7.8 (map 03 §8; `CoolProp-Tests-HS.cpp:39, :86`), class `Flash`: HS on CoolProp's production grids. Two
/// phase: Water, Nitrogen, R134a, MM, n-Pentane and Methane at 25 temperatures from the triple point + 1 K to Tc −
/// max(0.5 K, 1e-3 Tc), at Q 0.05, 0.25, 0.5, 0.75 and 0.95: the QT state. Single phase: those and CarbonDioxide and
/// Hydrogen on CoolProp's 20 × 20 (log p, T) grids, p from max(p at the triple point, 1e-6 pmax) to pmax and T from
/// the domain's lowest temperature to Tmax, at the cells' centres as every grid here (VERIFICATION.md §3.6): the PT
/// state wherever PT gives one, all but one (a solid Hydrogen state; see below). HS of each returns its phase, T and ρ within 1e-9
/// and its quality within 1e-8.
#[test]
fn hs_round_trips() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    let lin = |a: f64, b: f64, n: usize, k: usize| a + (b - a) * k as f64 / (n - 1) as f64;
    for name in ["Water", "Nitrogen", "R134a", "MM", "n-Pentane", "Methane"] {
        let fluid = registry.get(name).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let curve = record.superancillary_curve().unwrap();
        let (t_triple, t_c) = curve.t_range();
        for k in 0..25 {
            let t = lin(t_triple + 1.0, t_c - (1e-3 * t_c).max(0.5), 25, k);
            for q in [0.05, 0.25, 0.5, 0.75, 0.95] {
                let truth = fluid.state(Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())).unwrap();
                check.hs_round_trip(name, fluid, &truth);
            }
        }
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.compared, check.two_phase), (6 * 125, 6 * 125));
    let mut single = FlashCheck::default();
    for name in ["Water", "Nitrogen", "CarbonDioxide", "R134a", "MM", "n-Pentane", "Methane", "Hydrogen"] {
        let fluid = registry.get(name).unwrap();
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let limits = record.limits;
        let t_low = limits.t_triple().map_or(limits.t_min(), |t| t.max(limits.t_min()));
        let p_triple = record.superancillary_curve().map_or(0.0, |c| c.at_t(c.t_range().0).unwrap().bubble.p);
        let p_low = p_triple.max(1e-6 * limits.p_max());
        let centre = |a: f64, b: f64, k: usize| a + (b - a) * (k as f64 + 0.5) / 20.0;
        for i in 0..20 {
            let p = math::exp(centre(math::ln(p_low), math::ln(limits.p_max()), i));
            for j in 0..20 {
                let t = centre(t_low, limits.t_max(), j);
                if let Ok(truth) = fluid.state(Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())) {
                    single.hs_round_trip(name, fluid, &truth);
                }
            }
        }
    }
    // One PT truth, Hydrogen at 38.6 K and 782 MPa, lies far below its melting line, solid (melting lines arrive at
    // M8): there its EOS's isentrope folds, s rising again as T falls, and PS's highest root at that p is refused
    // under the triple point's temperature.
    let [hydrogen] = single.failures.as_slice() else { panic!("{:?}", single.report(20)) };
    assert!(
        hydrogen.starts_with("Hydrogen")
            && hydrogen.contains("782454277")
            && hydrogen.ends_with("BelowMinTemperature { t: 13.957, t_min: 13.957 })")
    );
    assert_eq!((single.compared, single.two_phase), (3199, 0));
}

/// PLAN.md M7.8 (map 01 R4: CoolProp has no SU), class `Flash`: SU of the core subset's 6 × 6 (p, T) truths returns
/// each one's phase, T and ρ within 1e-9.
#[test]
fn su_round_trips() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    for (path, text) in FLASH_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = fixture.header("fluid").and_then(|f| f.split(' ').next()).unwrap();
        check.su_round_trips(&fixture, registry.get(name).unwrap());
    }
    assert_eq!(check.report(20), None);
    assert_eq!(check.compared, 14 * 36);
}

/// PLAN.md M7.8: the issue-mined D+X and HS cases of CoolProp's `CoolProp-Tests-HSU_D.cpp` (map 03 §8), class `Flash`
/// where they round-trip a state. #2486: DH, DS and DU of the saturated liquid and vapour of Water, n-Propane, R134a,
/// CarbonDioxide and Nitrogen at 60 temperatures from the triple point + 0.5 K to Tc − 0.5 K. #2157, #1698, #1054: DU
/// and DS inside the dome of R1233zd(E), R32, R134a and R245fa, 40 temperatures × Q 0.01 to 0.9. #2154, #2173: the
/// (T, ρ) box around the critical point of eight fluids (T from 0.9 to 1.1 Tc, ρ from 0.5 to 2 ρc), every D+X pair; the
/// critical isochore crossed from 0.92 to 1.12 Tc by DU; the dome's edge 10 K to 1 mK below Tc. And single points:
/// Hydrogen at 31.46258141 kg/m³ and 2 391 760.261 J/kg (#2154: T between 233 and 414 K), Nitrogen at 313.305125
/// kg/m³ and 154 834.285193 J/kg (#1965, a singular matrix in CoolProp), R245fa's isolated D, S failures (#1054), and
/// Air's HS (#2022), including h = 435 kJ/kg, s = 2654 J/kg/K, where CoolProp still diverges: each solved, its inputs
/// reproduced within 1e-9.
#[test]
fn hsu_d_issue_cases() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut check = FlashCheck::default();
    let lin = |a: f64, b: f64, n: usize, k: usize| a + (b - a) * k as f64 / (n - 1) as f64;
    let qt = |q: f64, t: f64| Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap());
    let curve_of =
        |name: &str| phasekit_core::internal::record(&registry, name).unwrap().superancillary_curve().unwrap();
    for name in ["Water", "n-Propane", "R134a", "CarbonDioxide", "Nitrogen"] {
        let (fluid, (t_triple, t_c)) = (registry.get(name).unwrap(), curve_of(name).t_range());
        for k in 0..60 {
            for q in [0.0, 1.0] {
                let truth = fluid.state(qt(q, lin(t_triple + 0.5, t_c - 0.5, 60, k))).unwrap();
                check.dx_round_trip(name, fluid, &truth);
            }
        }
    }
    for name in ["R1233zd(E)", "R32", "R134a", "R245fa"] {
        let (fluid, (t_triple, t_c)) = (registry.get(name).unwrap(), curve_of(name).t_range());
        for k in 0..40 {
            for q in [0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9] {
                let truth = fluid.state(qt(q, lin(t_triple + 1.0, t_c - 1.0, 40, k))).unwrap();
                check.dx_round_trip(name, fluid, &truth);
            }
        }
    }
    let box_fluids = ["Water", "CarbonDioxide", "n-Propane", "R134a", "Nitrogen", "Hydrogen", "Helium", "Methane"];
    let rho_factors =
        [0.5, 0.75, 0.9, 0.95, 0.98, 0.99, 0.995, 0.999, 1.0, 1.001, 1.005, 1.01, 1.02, 1.05, 1.1, 1.25, 1.5, 2.0];
    let t_factors = [0.9, 0.95, 0.98, 0.99, 0.995, 1.0, 1.005, 1.01, 1.02, 1.05, 1.1];
    for name in box_fluids {
        let fluid = registry.get(name).unwrap();
        let c = phasekit_core::internal::record(&registry, name).unwrap().critical.unwrap();
        for (tf, rf) in t_factors.iter().flat_map(|tf| rho_factors.iter().map(move |rf| (*tf, *rf))) {
            if (tf, rf) != (1.0, 1.0) {
                let truth =
                    fluid.state(Input::dt(Density::molar(rf * c.rho).unwrap(), Temperature::new(tf * c.t).unwrap()));
                check.dx_round_trip(name, fluid, &truth.unwrap());
            }
        }
        for rf in [0.97, 0.99, 1.0, 1.01, 1.03] {
            for k in 0..40 {
                let t = lin(0.92 * c.t, 1.12 * c.t, 40, k);
                let truth = fluid.state(Input::dt(Density::molar(rf * c.rho).unwrap(), Temperature::new(t).unwrap()));
                check.dx_round_trip(name, fluid, &truth.unwrap());
            }
        }
        let (t_triple, t_c) = curve_of(name).t_range();
        for d_t in [10.0, 1.0, 0.1, 0.01, 0.001].into_iter().filter(|d_t| t_c - d_t > t_triple + 0.1) {
            for q in [0.1, 0.3, 0.5, 0.7, 0.9] {
                check.dx_round_trip(name, fluid, &fluid.state(qt(q, t_c - d_t)).unwrap());
            }
        }
    }
    assert_eq!(check.report(20), None);

    let mass = |name: &str, pair: Pair, x: f64, y: f64| {
        let fluid = registry.get(name).unwrap();
        let state = fluid.state(Input::new(pair, x, y, Basis::Mass).unwrap());
        state.unwrap_or_else(|e| panic!("{name} {pair:?} ({x}, {y}): {e:?}"))
    };
    let near = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-9;
    let hydrogen = mass("Hydrogen", Pair::DU, 31.46258141, 2_391_760.261);
    assert!(
        hydrogen.t() > 233.0 && hydrogen.t() < 414.0 && near(hydrogen.u(Basis::Mass), 2_391_760.261),
        "{hydrogen:?}"
    );
    let nitrogen = mass("Nitrogen", Pair::DU, 313.305125, 154_834.285193);
    assert!(near(nitrogen.u(Basis::Mass), 154_834.285193) && near(nitrogen.rho(Basis::Mass), 313.305125));
    for d in [17.04, 17.05, 17.048_396_706_125_065] {
        for s in [1617.4, 1617.5, 1_617.463_750_828_963] {
            let r245fa = mass("R245fa", Pair::DS, d, s);
            assert!(
                near(r245fa.s(Basis::Mass), s) && near(r245fa.rho(Basis::Mass), d),
                "R245fa ({d}, {s}): {r245fa:?}"
            );
        }
    }
    let air = [
        (429_667.306_4, 2735.612747),
        (430_867.419_3, 2732.980272),
        (427_500.0, 2740.0),
        (435_000.0, 2724.0),
        (435_000.0, 2654.0),
    ];
    for (h, s) in air {
        let state = mass("Air", Pair::HS, h, s);
        assert!(near(state.h(Basis::Mass), h) && near(state.s(Basis::Mass), s), "Air ({h}, {s}): {state:?}");
    }
}

/// VERIFICATION.md §3.2, §11.3: the `flash` files come from the pinned runner image, like every committed oracle
/// fixture.
#[test]
fn flash_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    for file in FLASH_CORE {
        assert_eq!(environment(file), smoke, "{}", file.0);
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/flash.csv (PLAN.md M6.9; D4, user decision 4; map 04 U4): the
/// six pseudo-pure fluids' `dome` rows, (ρ, T) between their QT densities at 4 × 4 cell centres of T and Q, flashed with
/// no phase imposed. Inside its dome CoolProp solves the pure-fluid VLE of the blend's EOS from the density ancillaries
/// (`HelmholtzEOSMixtureBackend.cpp:2447-2507`), as phasekit does: phase, Q, p, h, s and u as `dt_rows` checks them.
#[test]
fn pseudo_pure_in_dome_dt_follows_the_oracle_rule() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/flash.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let mut check = FlashCheck::default();
    for name in ["Air", "R404A", "R407C", "R410A", "R507A", "SES36"] {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let of = |row: usize| fixture.printed(row, "fluid") == Some(name);
        check.dt_rows_where(&fixture, name, (registry.get(name).unwrap(), &record), of);
    }
    assert_eq!(check.report(20), None);
    assert_eq!((check.compared, check.two_phase), (96, 92));
}
