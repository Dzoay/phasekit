//! L3 saturation (map 10 §8.5): the superancillary evaluated from the blobs against the oracle's own evaluation of the
//! same coefficients and against the multiprecision check points (VERIFICATION.md §3.5, §5), and its exact rescaling
//! under a gas-constant or reducing-density correction (VERIFICATION.md §7.3).

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::internal::{FluidRecord, IdealTerm};
use phasekit_core::{
    Basis, DataSet, Density, DomainPolicy, FlashOptions, Fluid, Input, Order, Phase, Registry, SatAccuracy,
    SaturationCurve, State, Temperature,
};
use phasekit_verify::eos::{Majorants, carried};
use phasekit_verify::term::IdealScale;
use phasekit_verify::{Cell, CheckError, Fixture, ToleranceClass, Window, fixture};

/// The superancillary of `name` from the embedded blob, under `data_set` and the hash gate.
fn curve(registry: &Registry, name: &str, data_set: DataSet) -> Box<dyn SaturationCurve> {
    let mut record = phasekit_core::internal::record(registry, name).unwrap();
    record.apply(data_set).unwrap();
    record.superancillary_curve().unwrap()
}

/// The text in `column` of `row`.
fn label<'a>(fixture: &Fixture<'a>, row: usize, column: &str) -> &'a str {
    let i = fixture.columns().iter().position(|c| *c == column).unwrap();
    match fixture.rows()[row].cells[i] {
        Cell::Text(text) => text,
        Cell::Num(_) | Cell::Blank => "",
    }
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
    for row in (0..fixture.rows().len()).filter(|&row| label(&fixture, row, "input") == "sa") {
        assert_eq!(label(&fixture, row, "status"), "ok", "row {row}");
        let name = label(&fixture, row, "fluid");
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
        let name = label(&fixture, row, "fluid");
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

/// VERIFICATION.md §3.2, §11.3: `all/sat.csv` comes from the pinned runner image, like every committed oracle fixture.
#[test]
fn sat_fixtures_name_the_pinned_generator_environment() {
    let environment = |(path, text): (&str, &str)| {
        let generator = Fixture::parse(path, text).unwrap().header("generator").unwrap_or_default();
        generator.split(' ').filter(|field| !field.starts_with("sha256=")).collect::<Vec<_>>().join(" ")
    };
    let smoke = environment(fixture!("coolprop-8.0.0/facts/smoke.csv"));
    assert_eq!(environment(fixture!("coolprop-8.0.0/all/sat.csv")), smoke);
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
    let (mut failures, mut checked) = (Vec::new(), 0);
    let mut cached: Option<(&str, FluidRecord, IdealScale)> = None;
    for row in (0..fixture.rows().len()).filter(|&row| label(&fixture, row, "input") == "T") {
        assert_eq!(label(&fixture, row, "status"), "ok", "row {row}");
        let name = label(&fixture, row, "fluid");
        if cached.as_ref().is_none_or(|(n, ..)| *n != name) {
            let record = phasekit_core::internal::record(&registry, name).unwrap();
            let ideal = IdealScale::new(&record).unwrap();
            cached = Some((name, record, ideal));
        }
        let Some((_, record, ideal)) = cached.as_ref() else { continue };
        let (fluid, t) = (registry.get(name).unwrap(), fixture.value(row, "T").unwrap());
        let (r, critical) = (record.eos.gas_constant, fluid.model().critical_point().unwrap());
        let (liquid, vapour) = saturated(fluid, &*record.superancillary_curve().unwrap(), t);
        let curves = record.caloric_view().unwrap().at(t).unwrap();
        let eos = fluid.model().helmholtz().unwrap();
        let columns =
            [("hL", &liquid, 0, false), ("hV", &vapour, 1, false), ("sL", &liquid, 2, true), ("sV", &vapour, 3, true)];
        for (column, state, curve, entropy) in columns {
            let rho = state.rho(Basis::Molar);
            let want = fixture.value(row, column).unwrap();
            let got = if entropy { state.s(Basis::Molar) } else { state.h(Basis::Molar) };
            let bundle = (eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two)).bundle().unwrap();
            let majorants = Majorants::at(record, ideal, t, rho);
            let relation = if entropy { "smolar" } else { "hmolar" };
            let spread = carried(relation, &bundle, &majorants, r, t, rho, fluid.info().molar_mass());
            let scale = want.abs().max(if entropy { r } else { r * t });
            let window = Window::at(t, rho, critical.t, critical.rho);
            let bound = ToleranceClass::Prop.bound_carried(scale, window, spread).unwrap();
            checked += 2;
            if let Err(e) = fixture.check_bound(row, column, got, bound) {
                failures.push(format!("{name} EOS: {e:?}"));
            }
            if (curves[curve] - want).abs() > caloric_fit(want, r, t, entropy) {
                failures.push(format!("{name} curve {column} at {t} K: {} against {want}", curves[curve]));
            }
        }
    }
    assert_eq!(checked, 130 * 8 * 4 * 2);
    assert_eq!(failures.iter().take(20).collect::<Vec<_>>(), Vec::<&String>::new(), "{} failures", failures.len());
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
