//! L3 saturation (map 10 §8.5): the superancillary evaluated from the blobs against the oracle's own evaluation of the
//! same coefficients and against the multiprecision check points (VERIFICATION.md §3.5, §5), its exact rescaling
//! under a gas-constant or reducing-density correction (VERIFICATION.md §7.3), and the QT and PQ flashes on it.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::HashMap;

use phasekit_core::internal::{FluidRecord, IdealTerm};
use phasekit_core::{
    Basis, CriticalPoint, DataSet, Density, DomainPolicy, Error, FlashOptions, Fluid, Input, Order, Phase, Pressure,
    Quality, Registry, RootPolicy, SatAccuracy, SaturationCurve, State, Strategy, Temperature,
};
use phasekit_verify::eos::{Majorants, carried};
use phasekit_verify::fixture::Kind;
use phasekit_verify::register::{RowKey, exempt_row};
use phasekit_verify::term::IdealScale;
use phasekit_verify::{CheckError, DIVERGENCES, Fixture, ToleranceClass, Window, fixture};

#[path = "common/sweep.rs"]
mod sweep;

use sweep::Failures;

/// The superancillary of `name` from the embedded blob, under `data_set` and the hash gate.
fn curve(registry: &Registry, name: &str, data_set: DataSet) -> Box<dyn SaturationCurve> {
    let mut record = phasekit_core::internal::record(registry, name).unwrap();
    record.apply(data_set).unwrap();
    record.superancillary_curve().unwrap()
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/sat.csv, the `sa` rows (PLAN.md M5.2; map 03 §8): p, ρ′ and ρ″
/// at 8 temperatures per fluid of the 130 with a superancillary, Θ = 1 − T/Tc log-spaced from 1e-7 to the triple
/// point, against `CP.SuperAncillary(json).eval_sat` on the same coefficients. Class `SaCoeff`. Water's curve spans
/// exactly its fitted range, triple point to numerical critical point.
#[test]
fn superancillary_matches_oracle_eval_sat() {
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut checked, mut headroom, mut fluids) = (Vec::new(), 0, 0.0_f64, Vec::new());
    for row in (0..fixture.rows().len()).filter(|&row| fixture.printed(row, "input").unwrap() == "sa") {
        assert_eq!(fixture.printed(row, "status").unwrap(), "ok", "row {row}");
        let name = fixture.printed(row, "fluid").unwrap();
        if fluids.last() != Some(&name) {
            fluids.push(name);
        }
        let sat = curve(&registry, name, DataSet::Parity).at_t(fixture.value(row, "T").unwrap()).unwrap();
        for (column, got) in [("p", sat.bubble.p), ("rhoL", sat.bubble.rho), ("rhoV", sat.dew.rho)] {
            checked += 1;
            let scale = fixture.value(row, column).unwrap().abs();
            match fixture.check_scaled(row, column, got, scale) {
                Ok(ratio) => headroom = headroom.max(ratio),
                Err(CheckError::Mismatch(m)) => failures.push(m.to_string()),
                Err(e) => failures.push(format!("{name}, row {row}, {column}: {e:?}")),
            }
        }
    }
    assert_eq!((fluids.len(), checked), (130, 130 * 8 * 3));
    assert_eq!(failures, Vec::<String>::new(), "headroom of the rest {headroom:.3}");
    assert_eq!(curve(&registry, "Water", DataSet::Parity).t_range(), (273.16, 647.095_999_999_987_3));
}

/// Arbiter: the multiprecision check points of CoolProp's fluid files, fixtures/mp/check-points.csv (PLAN.md M1.17,
/// M5.2): 3 per fluid of the 130, at Θ = 0.5, 0.3, 0.1. Class `SaFit`: within 4 × the point's own SA/mp misfit
/// (floor 1e-14), CoolProp's acceptance rule (map 10 §3). Three points sit an ulp or two below their fit's triple point
/// and are evaluated at it.
#[test]
fn superancillary_matches_the_check_points() {
    let (path, text) = fixture!("mp/check-points.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut moved) = (Vec::new(), Vec::new());
    for row in 0..fixture.rows().len() {
        let name = fixture.printed(row, "fluid").unwrap();
        let curve = curve(&registry, name, DataSet::Parity);
        // fastchebpure moved points below the triple point up to it; three came out 1-2 ulp below the fit, which the
        // curve refuses (D6), so they are evaluated at the triple point itself.
        let (t, t_min) = (fixture.value(row, "T").unwrap(), curve.t_range().0);
        if t < t_min {
            assert!(t_min - t <= 4.0 * f64::EPSILON * t_min, "{name}: {t} K is not at the triple point {t_min} K");
            moved.push(name);
        }
        let sat = curve.at_t(t.max(t_min)).unwrap();
        for (column, ratio, got) in [
            ("p", "p_sa_mp", sat.bubble.p),
            ("rhoL", "rhoL_sa_mp", sat.bubble.rho),
            ("rhoV", "rhoV_sa_mp", sat.dew.rho),
        ] {
            let bound = ToleranceClass::sa_fit(fixture.value(row, ratio).unwrap(), fixture.value(row, column).unwrap());
            if let Err(e) = fixture.check_bound(row, column, got, bound) {
                failures.push(format!("{name}: {e:?}"));
            }
        }
    }
    assert_eq!(fixture.rows().len(), 390);
    assert_eq!(moved, ["Nitrogen", "R114", "RC318"]);
    assert_eq!(failures, Vec::<String>::new());
}

/// VERIFICATION.md §7.3 (E14): a correction of R alone (R1234ze(E), DIV-0001) or of ρ_r alone (Nitrogen, DIV-0003)
/// keeps the superancillary `Exact`. Saturation is invariant in (τ, δ), so `Corrected` is `Parity` with ρ′ and ρ″ times
/// ρ_r′/ρ_r and p times (R′/R)(ρ_r′/ρ_r), bit for bit.
#[test]
fn rescaled_curves_apply_exact_factors() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let factors = [("R1234ze(E)", 8.314_462_1 / 8.314_472, 1.0), ("Nitrogen", 1.0, 11_183.9 / 11_183.901_464_580_624)];
    for (name, r, rho) in factors {
        let (parity, corrected) = (curve(&registry, name, DataSet::Parity), curve(&registry, name, DataSet::Corrected));
        assert_eq!((parity.accuracy(), corrected.accuracy()), (SatAccuracy::Exact, SatAccuracy::Exact), "{name}");
        assert_eq!(parity.t_range(), corrected.t_range());
        let (lo, hi) = parity.t_range();
        for k in 0..=20 {
            let t = lo + (hi - lo) * f64::from(k) / 20.0;
            let (p, c) = (parity.at_t(t).unwrap(), corrected.at_t(t).unwrap());
            assert_eq!(c.bubble.p.to_bits(), (p.bubble.p * (rho * r)).to_bits(), "{name} p at {t} K");
            assert_eq!(c.bubble.rho.to_bits(), (p.bubble.rho * rho).to_bits(), "{name} ρ′ at {t} K");
            assert_eq!(c.dew.rho.to_bits(), (p.dew.rho * rho).to_bits(), "{name} ρ″ at {t} K");
        }
    }
}

/// VERIFICATION.md §3.2, §11.3: `all/sat.csv` and the core subset's `sat` files come from the pinned runner image, like
/// every committed oracle fixture.
#[test]
fn sat_fixtures_name_the_pinned_generator_environment() {
    assert_eq!(
        phasekit_verify::on_the_pinned_image(std::iter::once(fixture!("coolprop-8.0.0/all/sat.csv")).chain(SAT_CORE)),
        Ok(15)
    );
}

/// The liquid and vapour states at T on the superancillary's densities, phase imposed: what a QT state is made of.
fn saturated(fluid: &Fluid, curve: &dyn SaturationCurve, t: f64) -> (State, State) {
    let sat = curve.at_t(t).unwrap();
    let at = |rho: f64, phase| {
        let opts = FlashOptions::new().with_phase(phase).with_domain(DomainPolicy::Extrapolate);
        fluid.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &opts).unwrap()
    };
    (at(sat.bubble.rho, Phase::Liquid), at(sat.dew.rho, Phase::Gas))
}

/// `Prop`'s bound (VERIFICATION.md §5) on an h (`entropy` false) or s near `want` at (T, ρ), with `Term` carried
/// through the relation.
fn eos_bound(
    fluid: &Fluid,
    (record, ideal): (&FluidRecord, &IdealScale),
    t: f64,
    rho: f64,
    entropy: bool,
    want: f64,
) -> f64 {
    let (eos, r) = (fluid.model().helmholtz().unwrap(), record.eos.gas_constant);
    let critical = fluid.model().critical_point().unwrap();
    let bundle = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
    let majorants = Majorants::at(record, ideal, t, rho);
    let relation = if entropy { "smolar" } else { "hmolar" };
    let spread = carried(relation, &bundle, &majorants, r, t, rho, fluid.info().molar_mass());
    let scale = want.abs().max(if entropy { r } else { r * t });
    ToleranceClass::Prop.bound_carried(scale, Window::at(t, rho, critical.t, critical.rho), spread).unwrap()
}

/// `CaloricFit` (VERIFICATION.md §5) of an h or u (`entropy` false) or an s at T.
fn caloric_fit(want: f64, r: f64, t: f64, entropy: bool) -> f64 {
    ToleranceClass::CaloricFit.bound(want.abs().max(if entropy { r } else { r * t })).unwrap()
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/all/sat.csv, the QT rows at Q = 0 (PLAN.md M5.2a; user decision
/// CC2): h′, h″, s′, s″ at 8 temperatures per fluid of the 130. The EOS at the superancillary's densities, which the
/// saturation flashes polish to and which the oracle evaluates too, matches at class `Prop` (with `Term` carried through
/// the relation, VERIFICATION.md §5); the caloric curves, starting points, match at `CaloricFit`.
#[test]
fn caloric_curves_match_oracle_sat_rows() {
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let fixture = Fixture::parse(path, text).unwrap();
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut failures, mut checked) = (Failures::default(), 0);
    let mut cache = None;
    let superancillary_qt = |row: &usize| {
        (fixture.printed(*row, "input").unwrap(), fixture.printed(*row, "path").unwrap()) == ("T", "superanc")
    };
    for row in (0..fixture.rows().len()).filter(superancillary_qt) {
        assert_eq!(fixture.printed(row, "status").unwrap(), "ok", "row {row}");
        let name = fixture.printed(row, "fluid").unwrap();
        let model = (registry.get(name).unwrap(), cached(&mut cache, &registry, name));
        checked += caloric_row(&fixture, row, model, &mut failures);
    }
    assert_eq!(checked, 130 * 8 * 4 * 2);
    assert_eq!(failures.iter().take(20).collect::<Vec<_>>(), Vec::<&String>::new(), "{} failures", failures.len());
}

/// The decoded record of `name` and its ideal-gas scale, from `cache` if it holds `name`'s, else decoded into it: a
/// fixture's rows of one fluid follow each other.
fn cached<'c, 'a>(
    cache: &'c mut Option<(&'a str, FluidRecord, IdealScale)>,
    registry: &Registry,
    name: &'a str,
) -> (&'c FluidRecord, &'c IdealScale) {
    if cache.as_ref().is_some_and(|(n, ..)| *n != name) {
        *cache = None;
    }
    let (_, record, ideal) = cache.get_or_insert_with(|| {
        let record = phasekit_core::internal::record(registry, name).unwrap();
        let ideal = IdealScale::new(&record).unwrap();
        (name, record, ideal)
    });
    (record, ideal)
}

/// One row of [`caloric_curves_match_oracle_sat_rows`]: h′, h″, s′ and s″ of the EOS at the superancillary's
/// densities within `Prop`, and of the caloric curves within `CaloricFit`, each failure into `failures`; returns the
/// cells compared.
fn caloric_row(
    fixture: &Fixture<'_>,
    row: usize,
    (fluid, (record, ideal)): (&Fluid, (&FluidRecord, &IdealScale)),
    failures: &mut Failures,
) -> usize {
    let (name, t) = (fixture.printed(row, "fluid").unwrap(), fixture.value(row, "T").unwrap());
    let r = record.eos.gas_constant;
    let (liquid, vapour) = saturated(fluid, &*record.superancillary_curve().unwrap(), t);
    let curves = record.caloric_view().unwrap().at(t).unwrap();
    let columns =
        [("hL", &liquid, 0, false), ("hV", &vapour, 1, false), ("sL", &liquid, 2, true), ("sV", &vapour, 3, true)];
    for (column, state, curve, entropy) in columns {
        let rho = state.rho(Basis::Molar);
        let want = fixture.value(row, column).unwrap();
        let got = if entropy { state.s(Basis::Molar) } else { state.h(Basis::Molar) };
        let bound = eos_bound(fluid, (record, ideal), t, rho, entropy, want);
        failures.ok(fixture.check_bound(row, column, got, bound), |e| format!("{name} EOS: {e:?}"));
        let off = (curves[curve] - want).abs() > caloric_fit(want, r, t, entropy);
        failures.check(!off, || format!("{name} curve {column} at {t} K: {} against {want}", curves[curve]));
    }
    2 * columns.len()
}

/// PLAN.md M2.11, M5.2a: another α⁰ offset, as CoolProp's IIR and NBP reference states write one, moves h and u by
/// R·T_r·Δa2 and s by −R·Δa1 (`CaloricStamp::gauge_to`): exactly so for the EOS (class `Identity` of the shifted and
/// unshifted values), and within `CaloricFit` for the curves plus that shift. CarbonDioxide, whose α⁰ carries an
/// offset term, at T across its range.
#[test]
fn gauge_shift_is_exact() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let record = phasekit_core::internal::record(&registry, "CarbonDioxide").unwrap();
    let mut shifted = record.clone();
    for term in &mut shifted.eos.ideal {
        if let IdealTerm::Offset { a1, a2, .. } = term {
            (*a1, *a2) = (*a1 + 0.75, *a2 - 0.5);
        }
    }
    let (a1, a2) = shifted.eos.offset();
    let (r, t_r) = (record.eos.gas_constant, record.eos.t_reducing);
    let curves = record.caloric.as_ref().unwrap();
    let gauge = curves.stamp.gauge_to(a1, a2, r, t_r).unwrap();
    assert_eq!((gauge.dh(), gauge.ds()), (r * t_r * -0.5, -r * 0.75));
    let curve = record.superancillary_curve().unwrap();
    let (base, moved) = (
        Fluid::new(std::sync::Arc::new(record.clone().compile().unwrap())),
        Fluid::new(std::sync::Arc::new(shifted.compile().unwrap())),
    );
    let (lo, hi) = curve.t_range();
    for k in 1..20 {
        let t = lo + (hi - lo) * f64::from(k) / 20.0;
        let ((l0, v0), (l1, v1)) = (saturated(&base, &*curve, t), saturated(&moved, &*curve, t));
        let values = curves.at(t).unwrap();
        let m = Basis::Molar;
        let pairs = [
            (l0.h(m), l1.h(m), values[0], false),
            (v0.h(m), v1.h(m), values[1], false),
            (l0.s(m), l1.s(m), values[2], true),
            (v0.s(m), v1.s(m), values[3], true),
            (l0.u(m), l1.u(m), values[4], false),
            (v0.u(m), v1.u(m), values[5], false),
        ];
        for (native, eos, fitted, entropy) in pairs {
            let shift = if entropy { gauge.ds() } else { gauge.dh() };
            let largest = native.abs().max(eos.abs()).max(shift.abs());
            assert!(
                (eos - native - shift).abs() <= ToleranceClass::Identity.bound(largest).unwrap(),
                "{t} K: {native} {eos} {shift}"
            );
            assert!(
                (fitted + shift - eos).abs() <= caloric_fit(eos, r, t, entropy),
                "{t} K: {fitted} + {shift} against {eos}"
            );
        }
    }
}

/// PLAN.md M5.2a (M2.11): DIV-0001's R correction of R1234ze(E) rescales the curves by R′/R exactly, and the rescaled
/// curves match the `Corrected` EOS at its superancillary densities within `CaloricFit`; a shape edit (one power-term
/// coefficient) makes them stale, and they refuse to answer (M7.7 then starts from the EOS).
#[test]
fn corrected_curves_rescale_or_go_stale() {
    let corrected_registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let raw = phasekit_core::internal::record(&corrected_registry, "R1234ze(E)").unwrap();
    let (mut parity, mut corrected) = (raw.clone(), raw.clone());
    parity.apply(DataSet::Parity).unwrap();
    corrected.apply(DataSet::Corrected).unwrap();
    let factor = 8.314_462_1 / 8.314_472;
    assert_eq!(corrected.caloric_freshness(), Some(phasekit_core::internal::CaloricFreshness::Rescaled { factor }));
    let fluid = corrected_registry.get("R1234ze(E)").unwrap();
    let curve = corrected.superancillary_curve().unwrap();
    let (lo, hi) = curve.t_range();
    let r = corrected.eos.gas_constant;
    for k in 1..20 {
        let t = lo + (hi - lo) * f64::from(k) / 20.0;
        let (p, c) = (parity.caloric_view().unwrap().at(t).unwrap(), corrected.caloric_view().unwrap().at(t).unwrap());
        assert_eq!(c.map(f64::to_bits), p.map(|v| (v * factor).to_bits()), "{t} K");
        let (l, v) = saturated(fluid, &*curve, t);
        let m = Basis::Molar;
        let exact = [l.h(m), v.h(m), l.s(m), v.s(m), l.u(m), v.u(m)];
        for (i, (got, want)) in c.iter().zip(exact).enumerate() {
            assert!(
                (got - want).abs() <= caloric_fit(want, r, t, i == 2 || i == 3),
                "{t} K, curve {i}: {got} against {want}"
            );
        }
    }
    let mut stale = parity.clone();
    stale.eos.power[0].n += 1e-12;
    assert_eq!(stale.caloric_freshness(), Some(phasekit_core::internal::CaloricFreshness::Stale));
    assert_eq!(stale.caloric_view(), None);
}

/// QT at quality `q` and temperature `t`.
fn qt(q: f64, t: f64) -> Input {
    Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())
}

/// PQ at pressure `p` and quality `q`.
fn pq(p: f64, q: f64) -> Input {
    Input::pq(Pressure::new(p).unwrap(), Quality::new(q).unwrap())
}

/// What [`ancillary_row`] counted.
#[derive(Debug, Default, PartialEq)]
struct AncillaryRows {
    rows: usize,
    /// Rows the oracle could not make.
    oracle_failed: usize,
    /// Rows phasekit's definition refuses (an ancillary with no root in its fitted range).
    refused: usize,
    cells: usize,
}

/// What [`q_rows`] counted and found.
#[derive(Debug, Default)]
struct QRows {
    qt: usize,
    pq: usize,
    exempt: usize,
    /// States outside the model's limits (flagged under `Extrapolate`).
    outside: usize,
    /// PQ rows whose p has several saturation temperatures.
    ambiguous: Vec<String>,
    /// A pseudo-pure fluid's QT and PQ rows (path `ancillary`).
    ancillary: AncillaryRows,
    failures: Failures,
}

/// The QT and PQ rows of `fixture` (`fluid_of(row)` the fluid of a row) against phasekit's flash (PLAN.md M6.8), every
/// row `ok` and on the superancillary's path, a QT row as [`qt_row`] checks it and a PQ row as [`pq_row`] does, with
/// the T of the QT row whose p it was given; a pseudo-pure fluid's rows as [`ancillary_row`] does. CoolProp's
/// superancillary flashes do not check the model's limits, so the rows are flashed under `Extrapolate`: a state outside
/// them is flagged, and refused by default (D6).
fn q_rows<'a>(fixture: &Fixture<'a>, fluid_of: impl Fn(usize) -> &'a str, registry: &Registry, out: &mut QRows) {
    let mut generated: HashMap<(&str, u64), f64> = HashMap::new();
    let mut cache = None;
    let q_input = |row: &usize| matches!(fixture.printed(*row, "input").unwrap(), "T" | "p");
    for row in (0..fixture.rows().len()).filter(q_input) {
        let (name, input) = (fluid_of(row), fixture.printed(row, "input").unwrap());
        let (status, path) = (fixture.printed(row, "status").unwrap(), fixture.printed(row, "path").unwrap());
        if path == "ancillary" {
            ancillary_row(fixture, row, (name, registry.get(name).unwrap()), out);
            continue;
        }
        let at = format!("{name} row {row} ({input})");
        if (status, path) != ("ok", "superanc") {
            out.failures.push(format!("{at}: {status}, {path}"));
            continue;
        }
        let (record, ideal) = cached(&mut cache, registry, name);
        let [q, t, p] = ["Q", "T", "p"].map(|column| fixture.value(row, column).unwrap());
        let q_row = QRow { fixture, row, at, name, fluid: registry.get(name).unwrap(), record, ideal, q, t, p };
        if input == "p" {
            pq_row(&q_row, generated.get(&(name, p.to_bits())).copied(), out);
        } else if qt_row(&q_row, out) {
            generated.insert((name, p.to_bits()), t);
        }
    }
}

/// The options of the QT and PQ rows' flashes: CoolProp's superancillary flashes do not check the model's limits.
const EXTRAPOLATE: FlashOptions = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);

/// A QT or PQ row on the superancillary's path, with its fluid's model.
struct QRow<'f, 'a> {
    fixture: &'f Fixture<'a>,
    row: usize,
    /// The fluid, row and input, for a failure's message.
    at: String,
    name: &'a str,
    fluid: &'f Fluid,
    record: &'f FluidRecord,
    ideal: &'f IdealScale,
    /// The row's Q, T and p.
    q: f64,
    t: f64,
    p: f64,
}

impl QRow<'_, '_> {
    /// QT of the row's fluid at quality `q` and temperature `t`, under `Extrapolate`.
    fn at_t(&self, q: f64, t: f64) -> Result<State, Error> {
        self.fluid.flash(qt(q, t), &EXTRAPOLATE)
    }

    /// The row's state, `flashed`, if it is the superancillary's; flagged exactly where it lies outside the model's
    /// limits, those counted. A failure is recorded in `out`.
    fn state(&self, flashed: Result<State, Error>, out: &mut QRows) -> Option<State> {
        let state = match flashed {
            Ok(state) if state.path().strategy == Strategy::Superancillary => state,
            other => {
                out.failures.push(format!("{}: {other:?}", self.at));
                return None;
            }
        };
        let limits = &self.record.limits;
        let outside = limits.check_t(state.t()).is_err() || limits.check_p(state.p()).is_err();
        out.outside += usize::from(outside);
        let flagged = state.is_extrapolated();
        out.failures
            .check(flagged == outside, || format!("{}: flagged {flagged}, outside the limits {outside}", self.at));
        Some(state)
    }
}

/// A QT row of [`q_rows`]: p, ρ′ and ρ″ within `SaCoeff`, and h and s within `Prop`, read off the states at Q = 0
/// and 1; returns whether its state is the superancillary's, so that a PQ row can take its T.
fn qt_row(row: &QRow<'_, '_>, out: &mut QRows) -> bool {
    let Some(state) = row.state(row.at_t(row.q, row.t), out) else { return false };
    out.qt += 1;
    let (fixture, m, t) = (row.fixture, Basis::Molar, row.t);
    let sides = [0.0, 1.0].map(|q| row.at_t(q, t).unwrap());
    for (column, got) in [("p", state.p()), ("rhoL", sides[0].rho(m)), ("rhoV", sides[1].rho(m))] {
        let scale = fixture.value(row.row, column).unwrap().abs();
        out.failures.ok(fixture.check_scaled(row.row, column, got, scale), |e| format!("{}: {e:?}", row.at));
    }
    for (column, side, entropy) in [("hL", 0, false), ("hV", 1, false), ("sL", 0, true), ("sV", 1, true)] {
        let (want, side) = (fixture.value(row.row, column).unwrap(), &sides[side]);
        let got = if entropy { side.s(m) } else { side.h(m) };
        let bound = eos_bound(row.fluid, (row.record, row.ideal), t, side.rho(m), entropy, want);
        out.failures.ok(fixture.check_bound(row.row, column, got, bound), |e| format!("{}: {e:?}", row.at));
    }
    true
}

/// A PQ row of [`q_rows`] (user decision PQ1), `t_qt` the T of the QT row whose p it was given: T within `SaCoeff` of
/// it (the root nearest it, where p has several), and its state phasekit's QT at that T, bit for bit, but for p, the
/// given one; the oracle's own PQ cells are DIV-0018's.
fn pq_row(row: &QRow<'_, '_>, t_qt: Option<f64>, out: &mut QRows) {
    let Some(t_qt) = t_qt else {
        out.failures.push(format!("{}: no QT row gave p = {}", row.at, row.p));
        return;
    };
    let nearest = EXTRAPOLATE.with_roots(RootPolicy::Nearest(t_qt));
    let Some(state) = row.state(row.fluid.flash(pq(row.p, row.q), &nearest), out) else { return };
    out.pq += 1;
    for column in ["T", "rhoL", "rhoV", "hL", "hV", "sL", "sV"] {
        let key = RowKey { t: row.t, input: Some("p"), two_phase: false };
        match exempt_row(DIVERGENCES, row.name, Kind::Sat, column, key) {
            Some("DIV-0018") => out.exempt += 1,
            other => out.failures.push(format!("{}: {column} exempt by {other:?}", row.at)),
        }
    }
    let within = (state.t() - t_qt).abs() <= ToleranceClass::SaCoeff.bound(t_qt).unwrap();
    out.failures.check(within, || format!("{}: T {} against the QT row's {t_qt}", row.at, state.t()));
    if let Err(Error::Ambiguous { roots }) = row.fluid.flash(pq(row.p, row.q), &EXTRAPOLATE) {
        out.ambiguous.push(format!("{}: {} roots", row.name, roots.as_slice().len()));
    }
    let same = row.at_t(row.q, state.t()).unwrap();
    let bits = |s: &State| [s.rho(Basis::Molar), s.h(Basis::Molar), s.s(Basis::Molar)].map(f64::to_bits);
    let p_close = (same.p() - row.p).abs() <= ToleranceClass::SaCoeff.bound(row.p).unwrap();
    let is_qt = bits(&same) == bits(&state) && state.p() == row.p && p_close;
    out.failures.check(is_qt, || format!("{}: {state:?} is not QT at its T, {same:?}", row.at));
}

/// A pseudo-pure fluid's QT or PQ row (path `ancillary`; PLAN.md M6.9, D4) against phasekit's flash (user decisions
/// PP1, PP2), each cell within `Prop` (1e-12 of max(|v|, floor), floors R·T and R; its near-critical bound where that
/// applies). QT: p is the side's pressure ancillary's, and the density, h and s those of the side Q names, the EOS's at
/// that (T, p). PQ: T, linear in Q between the sides' temperatures; at Q = 0 and Q = 1 the side's density, h and s are
/// phasekit's EOS at the oracle's T (CoolProp's inversion stops at 1e-10 K) and the given p. A row the oracle could not
/// make is counted, and one where phasekit's definition has no answer (an ancillary with no root in its fitted range,
/// D6) must be a `DomainError`, and is counted.
fn ancillary_row(fixture: &Fixture<'_>, row: usize, (name, fluid): (&str, &Fluid), out: &mut QRows) {
    let at = format!("{name} row {row}");
    out.ancillary.rows += 1;
    if fixture.printed(row, "status").unwrap() != "ok" {
        out.ancillary.oracle_failed += 1;
        return;
    }
    let [q, t, p] = ["Q", "T", "p"].map(|column| fixture.value(row, column).unwrap_or(f64::NAN));
    let given_t = fixture.printed(row, "input").unwrap() == "T";
    let flashed = fluid.state(if given_t { qt(q, t) } else { pq(p, q) });
    let Some(state) = ancillary_state(flashed, &at, out) else { return };
    let (m, r) = (Basis::Molar, fluid.model().helmholtz().unwrap().gas_constant());
    let cells = AncillaryCells { fixture, row, at: &at, critical: fluid.model().critical_point().unwrap() };
    let rho = state.rho(m);
    let (column, got) = if given_t { ("p", state.p()) } else { ("T", state.t()) };
    cells.compare(column, got, 0.0, (t, rho), out);
    if !given_t && q != 0.0 && q != 1.0 {
        return;
    }
    // The side Q names: QT's own; PQ's from the EOS at the oracle's T and the given p, seeded by phasekit's side.
    let liquid = q == 0.0;
    let Some(side) = (if given_t { Some(state) } else { eos_side(fluid, (t, p, rho), liquid, &at, out) }) else {
        return;
    };
    let (names, ts) = (if liquid { ["rhoL", "hL", "sL"] } else { ["rhoV", "hV", "sV"] }, (side.t(), side.rho(m)));
    cells.compare(names[0], side.rho(m), 0.0, ts, out);
    cells.compare(names[1], side.h(m), r * side.t(), ts, out);
    cells.compare(names[2], side.s(m), r, ts, out);
}

/// An ancillary row's state, `flashed`, if on the ancillaries' path; where phasekit's definition has no answer (a
/// `DomainError`), counted; any other outcome a failure recorded in `out`.
fn ancillary_state(flashed: Result<State, Error>, at: &str, out: &mut QRows) -> Option<State> {
    match flashed {
        Ok(state) if state.path().strategy == Strategy::Ancillary => Some(state),
        Err(Error::Domain(_)) => {
            out.ancillary.refused += 1;
            None
        }
        other => {
            out.failures.push(format!("{at}: {other:?}"));
            None
        }
    }
}

/// The saturated liquid (`liquid`) or vapour of a PQ row at Q = 0 or 1: the EOS's state at the oracle's T and the
/// given p, `(t, p)`, seeded by the density of phasekit's side, `rho`; where it has none, a failure recorded in `out`.
fn eos_side(fluid: &Fluid, (t, p, rho): (f64, f64, f64), liquid: bool, at: &str, out: &mut QRows) -> Option<State> {
    let eos = fluid.model().helmholtz().unwrap();
    let density = phasekit_core::internal::density_at_t_p(eos, t, p, rho, liquid);
    let rho = out.failures.ok(density, |e| format!("{at}: no density at the oracle's T: {e:?}"))?;
    let phase = if liquid { Phase::Liquid } else { Phase::Gas };
    let opts = FlashOptions::new().with_phase(phase).with_domain(DomainPolicy::Extrapolate);
    Some(fluid.flash(Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap()), &opts).unwrap())
}

/// The cells of one ancillary row, each compared within `Prop`.
struct AncillaryCells<'f, 'a> {
    fixture: &'f Fixture<'a>,
    row: usize,
    /// The fluid and row, for a failure's message.
    at: &'f str,
    /// The model's critical point, for the near-critical bounds.
    critical: CriticalPoint,
}

impl AncillaryCells<'_, '_> {
    /// `column`'s cell against `got`, within `Prop` of max(|want|, `floor`) at the side's (T, ρ); counted, and a
    /// failure recorded in `out` if outside.
    fn compare(&self, column: &str, got: f64, floor: f64, (t_side, rho_side): (f64, f64), out: &mut QRows) {
        let want = self.fixture.value(self.row, column).unwrap_or(f64::NAN);
        out.ancillary.cells += 1;
        let window = Window::at(t_side, rho_side, self.critical.t, self.critical.rho);
        let bound = ToleranceClass::Prop.bound_in(want.abs().max(floor), window).unwrap();
        let within = (got - want).abs() <= bound; // false for NaN too
        out.failures.check(within, || format!("{} {column}: {got} against {want} (bound {bound:e})", self.at));
    }
}

/// The core subset's `sat` files: the 12 fluids with a superancillary and the pseudo-pure Air and R410A (M6.9).
const SAT_CORE: [(&str, &str); 14] = [
    fixture!("coolprop-8.0.0/sat/Air.csv"),
    fixture!("coolprop-8.0.0/sat/Ammonia.csv"),
    fixture!("coolprop-8.0.0/sat/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/sat/HFE143m.csv"),
    fixture!("coolprop-8.0.0/sat/Helium.csv"),
    fixture!("coolprop-8.0.0/sat/Methanol.csv"),
    fixture!("coolprop-8.0.0/sat/n-Heptane.csv"),
    fixture!("coolprop-8.0.0/sat/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/sat/R1130(E).csv"),
    fixture!("coolprop-8.0.0/sat/R1234yf.csv"),
    fixture!("coolprop-8.0.0/sat/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/sat/R125.csv"),
    fixture!("coolprop-8.0.0/sat/R410A.csv"),
    fixture!("coolprop-8.0.0/sat/Water.csv"),
];

/// Oracle: CoolProp 8.0.0, the `sat` kind's QT and PQ rows (PLAN.md M6.8; map 03 §8): all/sat.csv, QT at Q = 0 and PQ
/// at Q = 1 at 8 temperatures per fluid of the 130 with a superancillary, and the core subset's sat/<Fluid>.csv, QT and
/// PQ at both Q at 25 temperatures, Θ = 1 − T/Tc log-spaced from 1e-7 to the triple point; each row as [`q_rows`]
/// checks it. 20 states lie outside their model's limits: MD4M's curve starts at 214.15 K, below its Tmin of 214.5 K,
/// R236EA's critical point (412.44 K) lies above its Tmax of 412 K and R161's (5.01 MPa) above its pmax of 5 MPa. At
/// PropyleneGlycol's p at 213 K the curve has a second temperature, 218.5 K, past the dip of DIV-0016's interval: the
/// default flash refuses it as `Ambiguous`, and CoolProp answers with the lower. The pseudo-pure fluids' rows land at
/// M6.9.
#[test]
fn sat_fixtures_match_oracle() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let mut rows = QRows::default();
    let (path, text) = fixture!("coolprop-8.0.0/all/sat.csv");
    let all = Fixture::parse(path, text).unwrap();
    q_rows(&all, |row| all.printed(row, "fluid").unwrap(), &registry, &mut rows);
    for (path, text) in SAT_CORE {
        let fixture = Fixture::parse(path, text).unwrap();
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        q_rows(&fixture, |_| name, &registry, &mut rows);
    }
    let failures = rows.failures.iter().take(20).collect::<Vec<_>>();
    assert_eq!(failures, Vec::<&String>::new(), "{} failures", rows.failures.len());
    let each = 130 * 8 + 12 * 50;
    assert_eq!((rows.qt, rows.pq, rows.exempt), (each, each, each * 7));
    assert_eq!((rows.outside, rows.ambiguous.as_slice()), (20, &["PropyleneGlycol: 2 roots".to_owned()] as &[String]));
    assert_eq!(rows.ancillary, AncillaryRows { rows: 487, oracle_failed: 1, refused: 9, cells: 1626 });
}

/// Map 11 §8, CoolProp's own smoke value (PLAN.md M6.8): R134a's h at 300 K and Q = 1 is 413265.6843372975 J/kg in
/// CoolProp 8.0.0 (`facts/smoke.csv`); phasekit's QT gives it within `Prop`.
#[test]
fn r134a_qt_smoke() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let h = registry.get("R134a").unwrap().state(qt(1.0, 300.0)).unwrap().h(Basis::Mass);
    let want = 413_265.684_337_297_5;
    assert!((h - want).abs() <= ToleranceClass::Prop.bound(want).unwrap(), "{h}");
}

/// Map 11 §8 (CoolProp's test, and its JS smoke test, VERIFICATION.md §10): Water boils at 101 325 Pa between
/// 373.124 K and 373.125 K.
#[test]
fn water_normal_boiling_point() {
    let t = Registry::embedded().unwrap().get("Water").unwrap().state(pq(101_325.0, 0.0)).unwrap().t();
    assert!((373.124..=373.125).contains(&t), "{t}");
}

/// Oracle: CoolProp 8.0.0 for R410A, a pseudo-pure fluid (PLAN.md M6.9; D4, user decision 4; 03-decision-log): QT at
/// 280 K gives the bubble pressure ancillary's 990480.516605891 Pa at Q = 0 and the dew one's 987288.0717853763 Pa at
/// Q = 1, and refuses Q = 0.5; PQ at 1 MPa gives T = 280.31657, 280.37003 and 280.42348 K at Q = 0, 0.5 and 1 (printed
/// to 1e-5 K), linear in Q. Each phase's density is the EOS's at its (T, p), path `Ancillary`.
#[test]
fn r410a_pseudo_pure_rows() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let r410a = registry.get("R410A").unwrap();
    let close = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-14;
    let bubble = r410a.state(qt(0.0, 280.0)).unwrap();
    let dew = r410a.state(qt(1.0, 280.0)).unwrap();
    assert!(close(bubble.p(), 990_480.516_605_891) && close(dew.p(), 987_288.071_785_376_3), "{bubble:?} {dew:?}");
    assert_eq!(bubble.path().strategy, Strategy::Ancillary);
    assert_eq!(r410a.state(qt(0.5, 280.0)), Err(Error::InvalidInput { quantity: "Q", value: 0.5 }));
    let t = [0.0, 0.5, 1.0].map(|q| r410a.state(pq(1e6, q)).unwrap().t());
    for (got, want) in t.iter().zip([280.316_57, 280.370_03, 280.423_48]) {
        assert!((got - want).abs() < 5e-6, "{t:?}");
    }
    assert!((t[1] - (t[0] + t[2]) / 2.0).abs() < 1e-12, "{t:?}");
}

/// ROT-084 (map 02 §6; PLAN.md M6.11, closing M6): QT at both of R13's critical temperatures. At the published 301.88 K,
/// where CoolProp without superancillaries throws, the dome still holds (ρ′ > ρ″): the model's own critical point is
/// 303.05 K. At the model's Tc, the top of its superancillary, QT is that point, ρ′ = ρ″, and p and ρ are the oracle's
/// computed ones (`crit` rows) within `Flash`'s near-critical bounds.
#[test]
fn qt_at_both_critical_temperatures() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let r13 = registry.get("R13").unwrap();
    let published = r13.info().published_critical().unwrap();
    let side = |q: f64, t: f64| r13.state(qt(q, t)).unwrap().rho(Basis::Molar);
    assert_eq!(published.t, 301.88);
    assert!(side(0.0, published.t) > 1.1 * side(1.0, published.t), "the dome at the published Tc");
    let top = phasekit_core::internal::record(&registry, "R13").unwrap().superancillary_curve().unwrap().t_range().1;
    let state = r13.state(qt(0.5, top)).unwrap();
    assert!((side(0.0, top) / side(1.0, top) - 1.0).abs() < 1e-14, "ρ′ = ρ″ at the model's Tc");
    let (path, text) = fixture!("coolprop-8.0.0/all/crit.csv");
    let crit = Fixture::parse(path, text).unwrap();
    let row = (0..crit.rows().len()).find(|&r| crit.printed(r, "fluid") == Some("R13")).unwrap();
    let cells = [("Tc_num", top, 1e-8), ("pc_num", state.p(), 1e-9), ("rhoc_num", state.rho(Basis::Molar), 1e-6)];
    for (column, got, bound) in cells {
        let want = crit.value(row, column).unwrap();
        assert!((got / want - 1.0).abs() <= bound, "{column}: {got} against {want}");
    }
}
