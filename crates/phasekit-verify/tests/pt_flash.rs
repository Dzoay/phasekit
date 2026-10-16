//! A mixture's PT flash (PLAN.md M13.8): Michelsen's stability test and the split, judged by map 04 §8's invariants on
//! ROT-181's and ROT-183's regression corpus and v8.0.0's Michelsen scenarios, and against the oracle's `mix_flash`.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::density::{self, Branch};
use phasekit_core::{Basis, DataSet, Error, FlashOptions, Fluid, Input, Order, Pair, Phase, Registry};
use phasekit_mix::Equilibrium;
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::{DIVERGENCES, Kind, Tolerance, ToleranceClass};

#[path = "common/mix_flash.rs"]
mod mix_flash;
use mix_flash::{binary, flash_rows, fugacity_differences, gibbs, mixture, root};

/// A mixture of the corpus, by its components and mole fractions.
type Feed = &'static [(&'static str, f64)];

/// ROT-181's and #3283's ternary: subcooled liquid at 2 MPa from 193.15 to 248.15 K (bubble pressure about 0.9 MPa).
const C1_C2_C3: Feed = &[("Methane", 0.05), ("Ethane", 0.90), ("n-Propane", 0.05)];
/// bd:ft05's five components (ROT-183): bubble point 90.7 K and dew point 244.433 K at 0.3 MPa.
const FT05: Feed =
    &[("Nitrogen", 0.3797), ("Methane", 0.3225), ("Ethane", 0.278), ("n-Butane", 0.0014), ("n-Pentane", 0.0184)];
const C1_C2: Feed = &[("Methane", 0.5), ("Ethane", 0.5)];
const C1_C2_85: Feed = &[("Methane", 0.85), ("Ethane", 0.15)];
const C1_C3: Feed = &[("Methane", 0.7), ("n-Propane", 0.3)];
const AIR: Feed = &[("Nitrogen", 0.79), ("Oxygen", 0.21)];
const N2_C1_C2_C3: Feed = &[("Nitrogen", 0.1), ("Methane", 0.5), ("Ethane", 0.25), ("n-Propane", 0.15)];
const C1_CO2: Feed = &[("Methane", 0.5), ("CarbonDioxide", 0.5)];
const C1_C2_CO2: Feed = &[("Methane", 0.3), ("Ethane", 0.3), ("CarbonDioxide", 0.4)];
/// Michelsen (1982a) Table 1's seven components.
const MICHELSEN: Feed = &[
    ("Methane", 0.9430),
    ("Ethane", 0.0270),
    ("n-Propane", 0.0074),
    ("n-Butane", 0.0049),
    ("n-Pentane", 0.0027),
    ("n-Hexane", 0.0010),
    ("Nitrogen", 0.0140),
];
/// #3066's natural gas, at pressures between its spinodals.
const GAS: Feed = &[
    ("Methane", 0.9254),
    ("Nitrogen", 0.007),
    ("CarbonDioxide", 0.008),
    ("Ethane", 0.048),
    ("n-Propane", 0.0085),
    ("n-Butane", 0.0014),
    ("n-Pentane", 0.0002),
    ("IsoButane", 0.0015),
];
const METHANOL_BENZENE: Feed = &[("Methanol", 0.54), ("Benzene", 0.46)];
const METHANOL_BENZENE_80: Feed = &[("Methanol", 0.80), ("Benzene", 0.20)];
/// #3448's water-rich feeds.
const CO2_WATER: Feed = &[("CarbonDioxide", 1e-6), ("Water", 1.0 - 1e-6)];
const N2_WATER: Feed = &[("Nitrogen", 1e-4), ("Water", 1.0 - 1e-4)];

/// What a state of the corpus is, as v8.0.0's tests (CoolProp-Tests-Michelsen.cpp, PTFlashMiddleRoot.cpp) assert it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Expect {
    /// One phase, denser than this (mol/m³).
    Above(f64),
    /// One phase, less dense than this.
    Below(f64),
    /// A vapour-liquid split with a vapour fraction in (lo, hi).
    Split(f64, f64),
}

/// The corpus: (mixture, T (K), p (Pa), what it is).
const CORPUS: &[(Feed, f64, f64, Expect)] = &[
    (C1_C2_C3, 193.15, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 212.5, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 222.5, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 223.15, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 225.0, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 235.0, 2e6, Expect::Above(14_000.0)),
    (C1_C2_C3, 248.15, 2e6, Expect::Above(14_000.0)),
    (FT05, 220.0, 3e5, Expect::Split(0.97, 0.99)),
    (FT05, 243.433, 3e5, Expect::Split(0.998, 0.9995)),
    (FT05, 274.433, 3e5, Expect::Below(1_000.0)),
    (C1_C2, 300.0, 1e5, Expect::Below(100.0)),
    (C1_C2, 150.0, 1e5, Expect::Split(0.0, 1.0)),
    (C1_C2, 100.0, 1e5, Expect::Above(20_000.0)),
    (C1_C2, 180.0, 1e6, Expect::Split(0.0, 1.0)),
    (C1_C2, 230.0, 4e6, Expect::Split(0.0, 1.0)),
    (C1_C2_85, 250.0, 1e6, Expect::Below(1_000.0)),
    (C1_C2_85, 120.0, 3e6, Expect::Above(20_000.0)),
    (C1_C2_85, 180.0, 2e6, Expect::Split(0.0, 1.0)),
    (C1_C3, 300.0, 1e5, Expect::Below(100.0)),
    (C1_C3, 150.0, 1e5, Expect::Split(0.0, 1.0)),
    (C1_C3, 100.0, 1e5, Expect::Above(20_000.0)),
    (C1_C3, 250.0, 3e6, Expect::Split(0.0, 1.0)),
    (AIR, 300.0, 1e5, Expect::Below(100.0)),
    (AIR, 80.0, 1e5, Expect::Split(0.0, 1.0)),
    (AIR, 75.0, 1e5, Expect::Above(20_000.0)),
    (N2_C1_C2_C3, 300.0, 1e5, Expect::Below(100.0)),
    (N2_C1_C2_C3, 145.0, 1e5, Expect::Split(0.0, 1.0)),
    (C1_CO2, 220.0, 3e6, Expect::Split(0.0, 1.0)),
    (C1_C2_CO2, 220.0, 2e6, Expect::Split(0.0, 1.0)),
    (MICHELSEN, 190.0, 4e6, Expect::Split(0.0, 1.0)),
    (MICHELSEN, 250.0, 1e6, Expect::Below(1_000.0)),
    (GAS, 290.15, 3.5e6, Expect::Below(3_000.0)),
    (GAS, 290.15, 3.45e6, Expect::Below(3_000.0)),
    (METHANOL_BENZENE_80, 308.15, 101_325.0, Expect::Above(9_000.0)),
    (CO2_WATER, 298.0, 1.5964e5, Expect::Above(50_000.0)),
    (N2_WATER, 320.0, 1e6, Expect::Above(50_000.0)),
];

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Corrected).unwrap()
}

fn feed(registry: &Registry, components: Feed) -> Fluid {
    phasekit_mix::mixture(registry, components, DataSet::Corrected).unwrap()
}

fn pt(fluid: &Fluid, t: f64, p: f64) -> Result<phasekit_core::State, Error> {
    fluid.flash(Input::new(Pair::PT, p, t, Basis::Molar)?, &FlashOptions::new())
}

/// Every split of the corpus (and methanol/benzene's liquid-liquid one), each with its mixture and T.
fn splits(registry: &Registry) -> Vec<(String, Fluid, f64, Equilibrium)> {
    let mut out = Vec::new();
    let states = CORPUS.iter().map(|(f, t, p, _)| (*f, *t, *p)).chain([(METHANOL_BENZENE, 308.15, 101_325.0)]);
    for (components, t, p) in states {
        let fluid = feed(registry, components);
        let e = mixture(&fluid).equilibrium(t, p).unwrap();
        if matches!(e, Equilibrium::Split { .. }) {
            out.push((format!("{components:?} at {t} K, {p} Pa"), fluid, t, e));
        }
    }
    out
}

/// Map 04 §8: at every split the components' fugacities are equal and the phases add up to the feed, within
/// `Identity`: 1e-12 of each equation's largest term.
#[test]
fn every_split_has_equal_fugacities_and_mass_balance() {
    let registry = registry();
    let splits = splits(&registry);
    for (name, fluid, t, e) in &splits {
        let mix = mixture(fluid);
        for (i, (d, bound)) in fugacity_differences(mix, *t, e).iter().enumerate() {
            assert!(d.abs() <= *bound, "{name}: ln f_{i} differs by {d:e}");
        }
        let Equilibrium::Split { beta, x, y, .. } = e else { continue };
        for (i, z) in mix.composition().iter().enumerate() {
            let total = (1.0 - beta) * x[i] + beta * y[i];
            assert!((total - z).abs() <= ToleranceClass::Identity.bound(1.0).unwrap(), "{name}: {i}: {total} for {z}");
        }
    }
    assert!(splits.len() >= 14, "{} splits", splits.len());
}

/// Map 04 §8: a split's Gibbs energy, (1 − β) Σ x_i ln(x_i φ_i^L) + β Σ y_i ln(y_i φ_i^V) over RT, lies below the
/// feed's on its stable root by more than `Identity`.
#[test]
fn a_split_lowers_the_gibbs_energy() {
    let registry = registry();
    for (name, fluid, t, e) in splits(&registry) {
        let mix = mixture(&fluid);
        let Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour } = &e else { continue };
        let p = phasekit_mix::MixtureModel::composition(mix.model(), t, *rho_vapour, y, Order::One).unwrap().pressure();
        let z = mix.composition();
        let feed = gibbs(mix, t, root(&registry, mix, t, p, z), z);
        let split = (1.0 - beta) * gibbs(mix, t, *rho_liquid, x) + beta * gibbs(mix, t, *rho_vapour, y);
        let bound = ToleranceClass::Identity.bound(feed.abs().max(1.0)).unwrap();
        assert!(split < feed - bound, "{name}: {split} against the feed's {feed}");
    }
}

/// ROT-181 (#3283): C1/C2/C3 at 2 MPa is single-phase on its liquid root from 193.15 to 248.15 K, the knife-edge
/// temperatures included (v8.0.0 returns ρ ≈ 6751 at 223.15 K, a root on a loop of the isotherm), within `Flash` of
/// the root core's density solve finds on the liquid branch.
#[test]
fn c1_c2_c3_is_single_phase_on_its_liquid_root() {
    let registry = registry();
    let fluid = feed(&registry, C1_C2_C3);
    for (_, t, p, _) in CORPUS.iter().filter(|(f, ..)| *f == C1_C2_C3) {
        let state = pt(&fluid, *t, *p).unwrap();
        let liquid = density::solve(fluid.model().helmholtz().unwrap(), *t, *p, Branch::Liquid, None).unwrap();
        let rho = state.rho(Basis::Molar);
        assert_eq!((state.phase(), state.quality()), (Phase::Liquid, None), "{t} K");
        assert!((rho - liquid).abs() <= ToleranceClass::Flash.bound(liquid).unwrap() && rho > 14_000.0, "{t} K: {rho}");
    }
}

/// The corpus (bd:ft05, #3283, #3066, #3448 and v8.0.0's Michelsen scenarios) takes the phases v8.0.0's tests assert:
/// one phase above or below a density, or a split with its vapour fraction in a range. Methanol/benzene at
/// x_MeOH = 0.54 splits into two liquids, which the PT flash refuses (MX4).
#[test]
fn the_corpus_takes_its_phases() {
    let registry = registry();
    let mut misses = Vec::new();
    for (components, t, p, expect) in CORPUS {
        let e = mixture(&feed(&registry, components)).equilibrium(*t, *p).unwrap();
        let ok = match (expect, &e) {
            (Expect::Above(lo), Equilibrium::Single { rho }) => rho > lo,
            (Expect::Below(hi), Equilibrium::Single { rho }) => rho < hi,
            (Expect::Split(lo, hi), Equilibrium::Split { beta, rho_liquid, rho_vapour, .. }) => {
                beta > lo && beta < hi && rho_liquid > rho_vapour
            }
            _ => false,
        };
        if !ok {
            misses.push(format!("{components:?} at {t} K, {p} Pa: {e:?}, expected {expect:?}"));
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
    let methanol = feed(&registry, METHANOL_BENZENE);
    let split = mixture(&methanol).equilibrium(308.15, 101_325.0).unwrap();
    let c = methanol.model().critical_point().unwrap();
    assert!(matches!(split, Equilibrium::Split { rho_vapour, .. } if rho_vapour > c.rho), "{split:?}");
    assert_eq!(pt(&methanol, 308.15, 101_325.0).err(), Some(Error::Unsupported { pair: Pair::PT }));
}

/// #3448: water-rich feeds take their liquid root, which rises from the model's largest density; the PT flash labels
/// them liquid.
#[test]
fn water_rich_feeds_take_their_liquid_root() {
    let registry = registry();
    for components in [CO2_WATER, N2_WATER] {
        let (t, p) = if components == CO2_WATER { (298.0, 1.5964e5) } else { (320.0, 1e6) };
        let fluid = feed(&registry, components);
        let state = pt(&fluid, t, p).unwrap();
        let liquid = density::solve(fluid.model().helmholtz().unwrap(), t, p, Branch::Liquid, None).unwrap();
        assert_eq!(state.phase(), Phase::Liquid, "{components:?}");
        assert!((state.rho(Basis::Molar) - liquid).abs() <= ToleranceClass::Flash.bound(liquid).unwrap());
    }
}

/// A two-phase PT state is its split: T, p, q = β, the bulk density 1/((1 − β)/ρ′ + β/ρ″) and the mass quality from
/// each phase's molar mass.
#[test]
fn a_two_phase_state_is_its_split() {
    let registry = registry();
    let fluid = feed(&registry, C1_C2);
    let (t, p) = (180.0, 1e6);
    let Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour } = mixture(&fluid).equilibrium(t, p).unwrap() else {
        panic!("C1/C2 at 180 K and 1 MPa splits")
    };
    let state = pt(&fluid, t, p).unwrap();
    let bulk = 1.0 / ((1.0 - beta) / rho_liquid + beta / rho_vapour);
    let flash = |want: f64| ToleranceClass::Flash.bound(want.abs()).unwrap();
    assert_eq!((state.phase(), state.t(), state.p()), (Phase::TwoPhase, t, p));
    assert!((state.quality().unwrap() - beta).abs() <= flash(beta));
    assert!((state.rho(Basis::Molar) - bulk).abs() <= flash(bulk));
    let masses = ["Methane", "Ethane"].map(|n| registry.get(n).unwrap().info().molar_mass());
    let molar_mass = |x: &[f64]| x.iter().zip(masses).map(|(x, m)| x * m).sum::<f64>();
    let quality = beta * molar_mass(&y) / ((1.0 - beta) * molar_mass(&x) + beta * molar_mass(&y));
    assert!((state.mass_quality().unwrap() - quality).abs() <= 1e-6, "{:?} against {quality}", state.mass_quality());
}

/// What a `mix_flash` row compared as.
enum Row {
    Single,
    Split,
    LiquidLiquid,
}

/// The row `o` against phasekit's equilibrium at its (T, p): a single phase's density within `Flash`, a split's Q, bulk
/// density and fractions within `measured`; a liquid-liquid split must be refused by the PT flash.
fn compare(registry: &Registry, o: &mix_flash::FlashRow, measured: f64) -> Result<Row, String> {
    let fluid = binary(registry, &o.label, o.z1);
    let mix = mixture(&fluid);
    let off = |got: f64, want: f64| ((got - want) / want).abs();
    let at = format!("{} {} at {} K, {} Pa", o.label, o.z1, o.t, o.p);
    match mix.equilibrium(o.t, o.p) {
        Ok(Equilibrium::Single { rho }) if o.q == -1.0 => {
            let ok = off(rho, o.rho) <= ToleranceClass::Flash.bound(1.0).unwrap();
            ok.then_some(Row::Single).ok_or(format!("{at}: rho {rho} against {}", o.rho))
        }
        Ok(Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour }) if o.q > 0.0 => {
            // Only a lighter phase denser than half the reducing density can be a liquid: the critical point, which
            // the search finds once per mixture, decides only those.
            let dense = rho_vapour > 0.5 * mix.model().reducing_state(mix.composition()).1;
            if dense && fluid.model().critical_point().is_some_and(|c| rho_vapour > c.rho) {
                assert_eq!(pt(&fluid, o.t, o.p).err(), Some(Error::Unsupported { pair: Pair::PT }), "{at}");
                return Ok(Row::LiquidLiquid);
            }
            let bulk = 1.0 / ((1.0 - beta) / rho_liquid + beta / rho_vapour);
            let offsets = [off(beta, o.q), off(bulk, o.rho), off(x[0], o.x1), off(y[0], o.y1)];
            offsets.iter().all(|d| *d <= measured).then_some(Row::Split).ok_or(format!("{at}: {offsets:?}"))
        }
        other => Err(format!("{at}: {other:?} against Q {}", o.q)),
    }
}

/// Oracle: CoolProp 8.0.0's PT flash (fixtures/coolprop-8.0.0/all/mix_flash.csv) of ten library binaries at three
/// compositions, two temperatures and three pressures each, on `Parity`. A single phase's density is phasekit's within
/// `Flash`. A split's Q, bulk density and phase fractions are phasekit's within DIV-0059's measured bound (v8.0.0's
/// split converges to 1e-9). DIV-0057's spurious roots and DIV-0058's near-azeotropic rows are skipped, and so are
/// liquid-liquid splits, which the PT flash refuses (MX4).
#[test]
fn pt_flashes_match_the_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0059").unwrap();
    let measured = match entry.tolerance {
        Some(Tolerance::Relative(x)) => x,
        _ => f64::NAN,
    };
    let (mut single, mut split, mut skipped, mut liquid_liquid, mut misses) = (0, 0, 0, 0, Vec::new());
    for o in flash_rows() {
        let key = RowKey { t: o.t, input: None, two_phase: o.q > 0.0, p: Some(o.p) };
        if exempt_row(DIVERGENCES, &o.label, Kind::MixFlash, "*", key).is_some() {
            skipped += 1;
            continue;
        }
        match compare(&registry, &o, measured) {
            Ok(Row::Single) => single += 1,
            Ok(Row::Split) => split += 1,
            Ok(Row::LiquidLiquid) => liquid_liquid += 1,
            Err(miss) => misses.push(miss),
        }
    }
    assert!(misses.is_empty(), "{} misses: {misses:#?}", misses.len());
    assert!(
        single >= 100 && split >= 50 && skipped == 5 && liquid_liquid >= 1,
        "{single} {split} {skipped} {liquid_liquid}"
    );
}

/// ROT-009: a single phase's label comes from the mixture's critical point (Methane/Ethane at 50/50: 262.5 K,
/// 6.76 MPa): at or above T_c supercritical above p_c and a supercritical gas below it; below T_c a supercritical
/// liquid above p_c and a liquid below it where ρ > ρ_c, a gas where ρ < ρ_c.
#[test]
fn single_phases_take_their_labels_from_the_critical_point() {
    let registry = registry();
    let fluid = feed(&registry, C1_C2);
    let c = fluid.model().critical_point().unwrap();
    assert!((c.t - 262.5).abs() < 0.1 && (c.p - 6.76e6).abs() < 0.01e6, "{c:?}");
    let cases = [
        (300.0, 10e6, Phase::Supercritical),
        (300.0, 1e6, Phase::SupercriticalGas),
        (200.0, 10e6, Phase::SupercriticalLiquid),
        (200.0, 3e6, Phase::Liquid),
        (250.0, 1e6, Phase::Gas),
    ];
    for (t, p, phase) in cases {
        let state = pt(&fluid, t, p).unwrap();
        let rho = state.rho(Basis::Molar);
        assert_eq!(state.phase(), phase, "{t} K, {p} Pa: rho {rho}");
        if t < c.t {
            assert_eq!(rho > c.rho, matches!(phase, Phase::Liquid | Phase::SupercriticalLiquid), "{t} K, {p} Pa");
        }
    }
}
