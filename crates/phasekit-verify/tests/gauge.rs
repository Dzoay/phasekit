//! Reference states as gauges at the boundary (VERIFICATION.md §8.4; map 15 §8; PLAN.md M5.4): any two gauges give the
//! same state but for exact shifts of h, s, u, g and a, and a custom reference state hits its anchor.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{
    Basis, DataSet, Density, Enthalpy, Entropy, Fluid, Gauge, Input, ReferenceState, Registry, Temperature,
};
use phasekit_verify::{ToleranceClass, gauge_invariance};

/// DT inputs spread over a fluid's domain that its handle flashes: gas, compressed liquid, inside the dome and
/// supercritical, from its superancillary's densities.
fn inputs(registry: &Registry, name: &str) -> Vec<Input> {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let curve = record.superancillary_curve().unwrap();
    let (lo, hi) = curve.t_range();
    let dt = |rho: f64, t: f64| Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
    let mut inputs = Vec::new();
    for k in 1..5 {
        let t = lo + (hi - lo) * f64::from(k) / 5.0;
        let sat = curve.at_t(t).unwrap();
        inputs.extend([
            dt(sat.dew.rho / 2.0, t),
            dt(sat.bubble.rho * 1.01, t),
            dt((sat.bubble.rho + sat.dew.rho) / 2.0, t),
        ]);
    }
    inputs.push(dt(record.critical.unwrap().rho, (hi + record.limits.t_max()) / 2.0)); // supercritical, below Tmax
    inputs
}

/// Map 15 §8, ROT-098 (user decision 2's reference states are gauges, never α⁰ edits): for 12 fluids with a
/// superancillary and two gauges each, every state is the native one bit for bit in T, p, ρ, cv, cp and w (or their
/// refusals in the dome), and h, u move by exactly Δh, s by Δs, g and a by Δh − TΔs (class `Identity`).
#[test]
fn gauge_invariance_on_real_fluids() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let fluids = [
        "Water",
        "CarbonDioxide",
        "Nitrogen",
        "Ammonia",
        "Helium",
        "Methanol",
        "R1234yf",
        "R1234ze(E)",
        "R125",
        "R1130(E)",
        "HFE143m",
        "n-Heptane",
    ];
    for name in fluids {
        let fluid = registry.get(name).unwrap();
        let inputs = inputs(&registry, name);
        for gauge in [Gauge::new(12_345.6, -7.8).unwrap(), Gauge::new(-3.0e4, 50.0).unwrap()] {
            assert_eq!(gauge_invariance(fluid, gauge, &inputs), Ok(()), "{name} in {gauge:?}");
            let regauged = fluid.with_gauge(Gauge::new(1.0, 1.0).unwrap());
            assert_eq!(gauge_invariance(&regauged, gauge, &inputs), Ok(()), "{name} from another gauge");
        }
    }
}

/// Map 01 §8, class `RefAnchor` (1e-8 absolute in J/kg and J/(kg K), the bound CoolProp's own reference-state tests use):
/// `ReferenceState::Custom` puts the given h and s at its anchor state, a compressed liquid or a two-phase state, for
/// any fluid; the gauge is the only change, so the anchor's T, p and ρ stay those of the native handle.
#[test]
fn custom_reference_state_hits_its_anchor() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let anchor = ToleranceClass::RefAnchor.bound(0.0).unwrap();
    for name in ["Water", "R1234ze(E)", "n-Heptane", "Helium"] {
        let fluid: &Fluid = registry.get(name).unwrap();
        for at in inputs(&registry, name).into_iter().take(3) {
            let (h, s) = (Enthalpy::mass(2.0e5).unwrap(), Entropy::mass(1.0e3).unwrap());
            let referenced = fluid.with_reference(ReferenceState::Custom { at, h, s }).unwrap();
            let (native, state) = (fluid.state(at).unwrap(), referenced.state(at).unwrap());
            assert!((state.h(Basis::Mass) - 2.0e5).abs() <= anchor, "{name}: h = {}", state.h(Basis::Mass));
            assert!((state.s(Basis::Mass) - 1.0e3).abs() <= anchor, "{name}: s = {}", state.s(Basis::Mass));
            assert_eq!((state.t(), state.p().to_bits()), (native.t(), native.p().to_bits()), "{name}");
        }
    }
}
