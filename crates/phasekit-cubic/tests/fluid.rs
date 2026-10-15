//! The cubic fluid (PLAN.md M11.4): its inputs come from the canonical fluid (ROT-216), its ideal part is the source's
//! (DIV-0046's proof, part 1), its names and critical point, and the core's flash on it.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, Constant, CriticalOrigin, DataSet, Derivs, FlashOptions, Fluid, Input, Order, Pair, Phase, Prop, Registry,
};
use phasekit_cubic::{Kind, R_CODATA, fluid, with_cubics};

const KINDS: [Kind; 3] = [Kind::Vdw, Kind::Srk, Kind::Pr];

fn embedded() -> &'static Registry {
    Registry::embedded().unwrap()
}

/// ROT-216: a cubic's T_c, p_c, ω and M are its source's published values, not a copied snapshot; its critical point
/// is the cubic's own (T_c, p_c, ρ_c = p_c/(Z_c R T_c), `CriticalOrigin::Model`), its published one those inputs; b =
/// Ω_b R T_c/p_c bounds its density, its virials are the residual's (B between −10 b and b at T_c), and it cites its
/// kind's paper.
#[test]
fn inputs_come_from_the_canonical_fluid() {
    for name in ["n-Propane", "Water", "R134a", "CycloPropane"] {
        let source = embedded().get(name).unwrap();
        let published = source.info().published_critical().unwrap();
        for kind in KINDS {
            let cubic = fluid(source, kind, None, DataSet::Corrected).unwrap();
            assert_eq!(cubic.info().name(), format!("{}::{name}", kind.prefix()));
            let c = cubic.model().critical_point().unwrap();
            assert_eq!((c.t, c.p, c.origin), (published.t, published.p, CriticalOrigin::Model), "{kind:?} {name}");
            assert!((c.p / (c.rho * R_CODATA * c.t) / kind.z_c() - 1.0).abs() <= 4.0 * f64::EPSILON);
            let constant = |f: &Fluid, k| f.constant(k).unwrap();
            for k in [Constant::Acentric, Constant::MolarMass] {
                assert_eq!(constant(&cubic, k), constant(source, k), "{kind:?} {name} {k:?}");
            }
            assert_eq!(constant(&cubic, Constant::GasConstant), R_CODATA);
            let b = kind.omegas(DataSet::Corrected).b * R_CODATA * published.t / published.p;
            let eos = cubic.model().helmholtz().unwrap();
            assert!((eos.rho_max(published.t) * b - (1.0 - 1e-6)).abs() <= 4.0 * f64::EPSILON, "{kind:?} {name}");
            let virials = eos.zero_density(published.t).unwrap();
            assert!(virials.b < b && virials.b > -10.0 * b, "{kind:?} {name}: B {} against b {b}", virials.b);
            let own = cubic.info().published_critical().unwrap();
            assert_eq!((own.t, own.p, own.origin), (published.t, published.p, CriticalOrigin::Published));
            let (bibkey, doi) = match kind {
                Kind::Vdw => ("vanderWaals-1873", None),
                Kind::Srk => ("Soave-CES-1972", Some("10.1016/0009-2509(72)80096-4")),
                _ => ("PengRobinson-IECF-1976", Some("10.1021/i160057a011")),
            };
            let source = cubic.info().source();
            assert_eq!((&*source.bibkey, source.doi.as_deref()), (bibkey, doi), "{kind:?}");
        }
    }
}

/// Every A_ij of a bundle to order 4, as bits.
fn bits(d: &Derivs) -> Vec<Option<u64>> {
    (0..=4).flat_map(|n| (0..=n).map(move |i| d.get(i, n - i).map(f64::to_bits))).collect()
}

/// DIV-0046's proof, part 1 (user decision CB1): for every cubic fluid of the embedded set the ideal part is the
/// source's `Derivs` bit for bit at the same (T, ρ), so its c_p⁰ is the source's times R_CODATA/R_source.
#[test]
fn ideal_part_equals_heos() {
    let registry = with_cubics(embedded(), &[Kind::Pr], DataSet::Corrected).unwrap();
    let mut checked = 0;
    for name in embedded().names() {
        let Ok(cubic) = registry.get(&format!("PR::{name}")) else { continue };
        let source = embedded().get(name).unwrap();
        let (heos, eos) = (source.model().helmholtz().unwrap(), cubic.model().helmholtz().unwrap());
        let limits = cubic.model().limits().unwrap();
        let t_at = |x: f64| limits.t_min() + x * (limits.t_max() - limits.t_min());
        for (t, rho) in [(t_at(0.95), 10.0), (t_at(0.5), 1.0), (t_at(0.3), 1000.0)] {
            let (want, got) = (heos.ideal(t, rho, Order::Four), eos.ideal(t, rho, Order::Four));
            assert_eq!(bits(&got), bits(&want), "{name} at {t} K");
            let input = Input::new(Pair::DT, rho.min(1.0), t, Basis::Molar).unwrap();
            let cp0 = |f: &Fluid| f.prop(&f.state(input).unwrap(), Prop::Cp0molar).unwrap();
            let ratio = R_CODATA / heos.gas_constant();
            assert!((cp0(cubic) / cp0(source) / ratio - 1.0).abs() <= 4.0 * f64::EPSILON, "{name} at {t} K");
        }
        checked += 1;
    }
    assert!(checked >= 120, "{checked} cubic fluids");
}

/// `with_cubics` registers each kind under CoolProp's prefixes, the source's aliases prefixed (`PR::Propane` is
/// `PR::n-Propane`), and leaves out the pseudo-pure blends.
#[test]
fn with_cubics_names_every_pure_fluid() {
    let registry = with_cubics(embedded(), &KINDS, DataSet::Corrected).unwrap();
    assert_eq!(registry.canonical_name("PR::Propane"), Some("PR::n-Propane"));
    assert_eq!(registry.canonical_name("srk::r290"), Some("SRK::n-Propane"));
    assert_eq!(registry.canonical_name("VDW::Water"), Some("VDW::Water"));
    assert_eq!(registry.canonical_name("Propane"), Some("n-Propane"), "the HEOS fluids stay");
    for blend in ["Air", "R404A", "R407C", "R410A", "R507A", "SES36"] {
        assert_eq!(registry.canonical_name(&format!("PR::{blend}")), None, "{blend}");
    }
    let pure = embedded().names().filter(|n| !["Air", "R404A", "R407C", "R410A", "R507A", "SES36"].contains(n)).count();
    let cubic = registry.names().filter(|n| n.contains("::")).count();
    assert_eq!(cubic, 3 * pure, "{cubic} cubic names for {pure} pure fluids");
}

/// The core's flash on a cubic fluid: PR n-Propane at 300 K and 1 bar is a gas near the ideal one, and SRK and PR Water
/// at 300 K and 1 atm is a liquid (ROT-009: v8.0.0 labels it gas).
#[test]
fn the_core_flashes_a_cubic_fluid() {
    let registry = with_cubics(embedded(), &[Kind::Srk, Kind::Pr], DataSet::Corrected).unwrap();
    let pt = |p: f64, t: f64| Input::new(Pair::PT, p, t, Basis::Molar).unwrap();
    let propane = registry.get("PR::Propane").unwrap();
    let gas = propane.flash(pt(1e5, 300.0), &FlashOptions::default()).unwrap();
    let z = gas.p() / (gas.rho(Basis::Molar) * R_CODATA * 300.0);
    assert!(gas.phase() == Phase::Gas && (0.95..1.0).contains(&z), "{gas:?}");
    for name in ["SRK::Water", "PR::Water"] {
        let water = registry.get(name).unwrap().flash(pt(101_325.0, 300.0), &FlashOptions::default()).unwrap();
        assert_eq!(water.phase(), Phase::Liquid, "{name}");
    }
}
