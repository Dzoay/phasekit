//! A mixture's bubble and dew points (PLAN.md M13.9): QT and PQ at Q = 0 and 1, judged by the equations they solve
//! (ROT-185), their fugacities (ROT-194), teqp's GERG-2008 isotherms, the oracle's `mix_sat` and the pseudo-pure names'
//! ancillaries.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Basis, DataSet, DomainError, Error, FlashOptions, Fluid, Input, Pair, Phase, Registry, math};
use phasekit_mix::{Given, MixtureModel, Saturation, Side};
use phasekit_verify::{DIVERGENCES, Fixture, Tolerance, ToleranceClass, fixture};

#[path = "common/mix_sat.rs"]
mod mix_sat;
use mix_sat::{fugacity_differences, mixture, phases, row_fluid, sat_rows};

fn registry() -> Registry {
    Registry::from_embedded(DataSet::Parity).unwrap()
}

/// The QT or PQ flash of `fluid` at Q with the other input `x` (T in K or p in Pa).
fn flash(fluid: &Fluid, pair: Pair, q: f64, x: f64) -> Result<phasekit_core::State, Error> {
    let input = match pair {
        Pair::QT => Input::new(Pair::QT, q, x, Basis::Molar)?,
        _ => Input::new(Pair::PQ, x, q, Basis::Molar)?,
    };
    fluid.flash(input, &FlashOptions::new())
}

/// Every oracle row phasekit solves: the row, its fluid and phasekit's point. Rows below the mixture's T_min (CO₂'s
/// triple point in CO₂/Methane; the blends' components') are skipped and counted.
fn solved(registry: &Registry) -> (Vec<(mix_sat::SatRow, Fluid, Saturation)>, usize) {
    let (mut out, mut below) = (Vec::new(), 0);
    for row in sat_rows() {
        let fluid = row_fluid(registry, &row.label, row.z1);
        if row.t < fluid.model().limits().unwrap().t_min() {
            below += 1;
            continue;
        }
        let given = if row.given_t { Given::T(row.t) } else { Given::P(row.p) };
        let s = mixture(&fluid).saturation(row.side(), given).unwrap();
        out.push((row, fluid, s));
    }
    (out, below)
}

/// ROT-185: every bubble and dew point ends on its equations, never on its step. At each of the oracle's rows phasekit
/// solves (the ten binaries' and the blends') each phase's density reproduces p by core's acceptance gate, and
/// ln K_i + ln φ_i^V − ln φ_i^L, recomputed from the point returned, is within `Identity` of 0.
#[test]
fn bubble_and_dew_residuals_meet_the_gate() {
    let registry = registry();
    let (points, _) = solved(&registry);
    for (row, fluid, s) in &points {
        let mix = mixture(fluid);
        let at = format!("{} {} {:?} at {} K, {} Pa", row.label, row.z1, row.side(), s.t, s.p);
        let ((x, rho_l), (y, rho_v)) = phases(mix, row.side(), s);
        for (fractions, rho) in [(&x, rho_l), (&y, rho_v)] {
            let c = mix.model().composition(s.t, rho, fractions, phasekit_core::Order::One).unwrap();
            let scale = (rho * mix.model().gas_constant() * s.t * (1.0 + (c.z() - 1.0).abs())).max(-c.dp_dv() / rho);
            assert!((c.pressure() - s.p).abs() <= 1e-12 * scale, "{at}: p({rho}) = {}", c.pressure());
        }
        for (i, (d, bound)) in fugacity_differences(mix, s.t, s.p, (&x, rho_l), (&y, rho_v)).iter().enumerate() {
            assert!(d.abs() <= *bound, "{at}: ln f_{i} differs by {d:e}");
        }
    }
    assert!(points.len() >= 652, "{}", points.len());
}

/// ROT-194: at Q = 0 and 1 the state is the saturated split, and the fugacities of its two phases are defined and equal
/// (v8.0.0's `fugacity_coefficient` throws there). For a binary, a ternary blend and a GERG-2008 gas, QT and PQ at the
/// bubble and the dew point: Q and the mass quality are 0 or 1, the bulk density is the feed's, which is the liquid
/// at Q = 0 and the vapour at Q = 1, and every component's fugacity x_i φ_i p is finite, positive and the same in both
/// phases within `Identity`; the feed's chemical potentials are finite.
#[test]
fn fugacity_at_q_0_and_1() {
    let registry = registry();
    let gas = [("Methane", 0.85), ("Ethane", 0.07), ("n-Propane", 0.03), ("Nitrogen", 0.03), ("CarbonDioxide", 0.02)];
    let fluids = [
        (phasekit_mix::mixture(&registry, &[("R32", 0.6), ("R125", 0.4)], DataSet::Parity).unwrap(), 250.0, 1e6),
        (row_fluid(&registry, "R407C.mix", f64::NAN), 260.0, 1e6),
        (phasekit_mix::gerg2008(&registry, &gas).unwrap(), 180.0, 2e6),
    ];
    let mut checked = 0;
    for (fluid, t, p) in &fluids {
        for (q, side) in [(0.0, Side::Bubble), (1.0, Side::Dew)] {
            saturated_split(fluid, q, side, (Pair::QT, Given::T(*t), *t));
            saturated_split(fluid, q, side, (Pair::PQ, Given::P(*p), *p));
            checked += 2;
        }
    }
    assert_eq!(checked, 12);
}

/// The QT or PQ state of `fluid` at Q and the input `x` is its saturation point's split, with equal fugacities in its
/// two phases (`fugacity_at_q_0_and_1`).
fn saturated_split(fluid: &Fluid, q: f64, side: Side, (pair, given, x): (Pair, Given, f64)) {
    let mix = mixture(fluid);
    let state = flash(fluid, pair, q, x).unwrap();
    let s = mix.saturation(side, given).unwrap();
    let at = format!("{:?} {pair:?} at Q {q}", mix.composition());
    assert_eq!((state.t(), state.p(), state.phase()), (s.t, s.p, Phase::TwoPhase), "{at}");
    assert_eq!((state.quality(), state.mass_quality()), (Some(q), Some(q)), "{at}");
    assert!((state.rho(Basis::Molar) / s.rho - 1.0).abs() <= 4.0 * f64::EPSILON, "{at}");
    let ((xl, rho_l), (yv, rho_v)) = phases(mix, side, &s);
    assert!(rho_l > rho_v, "{at}: {rho_l} {rho_v}");
    for (fractions, rho) in [(&xl, rho_l), (&yv, rho_v)] {
        let phi = mix_sat::ln_phi(mix, s.t, s.p, rho, fractions);
        assert!(phi.iter().zip(fractions.iter()).all(|(l, x)| (x * math::exp(*l) * s.p).is_finite()), "{at}");
    }
    let differences = fugacity_differences(mix, s.t, s.p, (&xl, rho_l), (&yv, rho_v));
    assert!(differences.iter().all(|(d, bound)| d.abs() <= *bound), "{at}: {differences:?}");
    assert!(mix.chemical_potentials(s.t, s.rho).unwrap().iter().all(|mu| mu.is_finite()), "{at}");
}

/// teqp 0.23.2's vapour-liquid isotherms of GERG-2008's methane/ethane at 200 and 250 K (`trace_VLE_isotherm_binary`,
/// polished; differential, never an arbiter): at each of their liquid compositions phasekit's bubble point at the
/// isotherm's T has teqp's p, vapour fraction and both densities within `Flash`. The 250 K trace ends at the mixture's
/// critical point (x₁ = y₁ = 0.618), where the incipient phase is the feed: phasekit refuses it as the trivial
/// solution.
#[test]
fn gerg_methane_ethane_isotherms_match_teqp() {
    let (path, text) = fixture!("teqp-0.23.2/gerg/vle.csv");
    let table = Fixture::parse(path, text).unwrap();
    let registry = registry();
    let (mut misses, mut critical) = (Vec::new(), 0);
    for row in 0..table.rows().len() {
        let value = |column: &str| table.value(row, column).unwrap();
        let (t, x1, y1) = (value("T"), value("x1"), value("y1"));
        let fluid = phasekit_mix::gerg2008(&registry, &[("Methane", x1), ("Ethane", 1.0 - x1)]).unwrap();
        let found = mixture(&fluid).saturation(Side::Bubble, Given::T(t));
        if (x1 - y1).abs() < 1e-10 {
            assert!(found.is_err(), "{t} K, x1 {x1}: {found:?}");
            critical += 1;
            continue;
        }
        let s = found.unwrap();
        for (column, got) in
            [("p", s.p), ("y1", s.incipient[0]), ("rho_liquid", s.rho), ("rho_vapour", s.rho_incipient)]
        {
            if let Err(e) = table.check_scaled(row, column, got, got.abs()) {
                misses.push(format!("{t} K, x1 {x1} {column}: {e:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
    assert!(critical == 1 && table.rows().len() >= 30, "{critical} of {}", table.rows().len());
}

/// Oracle: CoolProp 8.0.0's bubble and dew points (fixtures/coolprop-8.0.0/all/mix_sat.csv) of ten library binaries at
/// three compositions and of the predefined blends, QT and PQ, on `Parity`. phasekit's T, p, ρ′ and ρ″, and a binary's
/// incipient fraction, are the oracle's within DIV-0060's measured bound (v8.0.0's Newton stops on its step). The 44
/// rows below their mixture's T_min (19 of CO₂/Methane's, under CO₂'s triple point, 216.59 K; 4 more of the binaries';
/// 21 of the blends') are skipped.
#[test]
fn bubble_and_dew_points_match_the_oracle() {
    let registry = registry();
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0060").unwrap();
    let measured = match entry.tolerance {
        Some(Tolerance::Relative(x)) => x,
        _ => f64::NAN,
    };
    let (points, below) = solved(&registry);
    let mut misses = Vec::new();
    for (row, fluid, s) in &points {
        let ((x, rho_l), (y, rho_v)) = phases(mixture(fluid), row.side(), s);
        let mut pairs = vec![(s.t, row.t), (s.p, row.p), (rho_l, row.rho_liquid), (rho_v, row.rho_vapour)];
        if row.z1.is_finite() {
            pairs.extend([(x[0], row.x1), (y[0], row.y1)]);
        }
        let offsets: Vec<f64> = pairs.iter().map(|(got, want)| ((got - want) / want).abs()).collect();
        if offsets.iter().any(|d| *d > measured) {
            misses.push(format!("{} {} {:?} at {} K, {} Pa: {offsets:?}", row.label, row.z1, row.side(), row.t, row.p));
        }
    }
    assert!(misses.is_empty(), "{} misses: {misses:#?}", misses.len());
    let binaries = points.iter().filter(|(row, ..)| row.z1.is_finite()).count();
    assert!(binaries >= 215 && points.len() - binaries >= 437 && below == 44, "{binaries} {} {below}", points.len());
}

/// The predefined blends' bubble and dew pressures against the pseudo-pure names' ancillaries, which were fitted to
/// them (Lemmon 2003 for R404A, R407C, R410A and R507A; Lemmon et al. 2000 for Air): QT at Q = 0 and 1 at eleven
/// temperatures from the higher of the two T_min to 0.95 T_c, within 3e-4 of the pseudo-pure p for the refrigerants
/// (measured 2.2e-4) and 4e-3 for Air (3.3e-3); the ancillaries state 5 % and 1 %.
#[test]
fn blends_meet_their_pseudo_pure_ancillaries() {
    let registry = registry();
    for (name, bound) in [("R404A", 3e-4), ("R407C", 3e-4), ("R410A", 3e-4), ("R507A", 3e-4), ("Air", 4e-3)] {
        let blend = row_fluid(&registry, &format!("{name}.mix"), f64::NAN);
        let pure = registry.get(name).unwrap();
        let t_min = blend.model().limits().unwrap().t_min().max(pure.model().limits().unwrap().t_min());
        let t_max = 0.95 * pure.model().critical_point().unwrap().t;
        for k in 0..=10 {
            let t = t_min + (t_max - t_min) * f64::from(k) / 10.0;
            for q in [0.0, 1.0] {
                let (a, b) = (flash(&blend, Pair::QT, q, t).unwrap(), flash(pure, Pair::QT, q, t).unwrap());
                assert!((a.p() / b.p() - 1.0).abs() <= bound, "{name} at {t} K, Q {q}: {} against {}", a.p(), b.p());
            }
        }
    }
}

/// QT and PQ serve Q = 0 and 1 only until M13.10: a Q inside (0, 1) is `Unsupported`. PQ at QT's bubble and dew
/// pressures returns QT's temperature within `Flash`. A given T below the mixture's T_min is refused before the solve.
#[test]
fn quality_inputs_are_the_saturation_points() {
    let registry = registry();
    let fluid = phasekit_mix::mixture(&registry, &[("CarbonDioxide", 0.5), ("Methane", 0.5)], DataSet::Parity).unwrap();
    for q in [0.0, 1.0] {
        let p = flash(&fluid, Pair::QT, q, 230.0).unwrap().p();
        let t = flash(&fluid, Pair::PQ, q, p).unwrap().t();
        assert!((t - 230.0).abs() <= ToleranceClass::Flash.bound(230.0).unwrap(), "Q {q}: {t} K at {p} Pa");
        for (pair, x) in [(Pair::QT, 230.0), (Pair::PQ, p)] {
            assert_eq!(flash(&fluid, pair, 0.5, x).err(), Some(Error::Unsupported { pair }));
        }
    }
    let t_min = fluid.model().limits().unwrap().t_min();
    let below = flash(&fluid, Pair::QT, 0.0, 200.0).err();
    assert_eq!(below, Some(Error::Domain(DomainError::BelowMinTemperature { t: 200.0, t_min })));
    let caps = fluid.model().capabilities();
    assert!(caps.contains(Pair::QT) && caps.contains(Pair::PQ) && caps.contains(Pair::PT));
}
