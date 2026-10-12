//! The caloric curves (PLAN.md M5.2a): the shipped curves against the EOS they were fitted to.

use super::*;
use phasekit_core::{Basis, Density, DomainPolicy, FlashOptions, Fluid, Input, Phase, SaturationCurve, Temperature};

/// PLAN.md M5.2a (ROT-027; user decision CC2): the shipped caloric curves of the 130 fluids with a superancillary,
/// decoded from their blobs, against the `Parity` EOS at (T, ρ_SA(T)) at two points of every piece where the fit
/// has no node (between Chebyshev-Lobatto nodes 2 and 3, and 8 and 9). Class `CaloricFit`: 2e-6 of max(|v|, floor),
/// floor R·T for h and u, R for s.
#[test]
fn caloric_curves_match_the_eos_between_nodes() {
    let (mut fluids, mut worst) = (0, 0.0_f64);
    for source in sources().iter().filter(|s| s.fluid.eos[0].superancillary.is_some()) {
        worst = worst.max(worst_between_nodes(source));
        fluids += 1;
    }
    assert_eq!(fluids, 130);
    assert!(worst > 1e-7, "the measured worst, 1.1e-6, sets CaloricFit: {worst:e}");
}

/// One fluid of [`caloric_curves_match_the_eos_between_nodes`]: its shipped curves at the two points of every piece,
/// as [`worst_at`] checks them; returns the largest miss.
fn worst_between_nodes(source: &Source) -> f64 {
    let shipped = FluidRecord::decode(&record::to_record(source).unwrap().encode()).unwrap();
    let sa = shipped.superancillary_curve().unwrap();
    let breaks = shipped.caloric.as_ref().unwrap().breaks.clone();
    let fluid = Fluid::new(std::sync::Arc::new(shipped.clone().compile().unwrap()));
    let node = |k: f64| math::cos(std::f64::consts::PI * k / 12.0);
    let mut worst = 0.0_f64;
    for w in breaks.windows(2) {
        for x in [(node(2.0) + node(3.0)) / 2.0, (node(8.0) + node(9.0)) / 2.0] {
            let t = w[0] + (x + 1.0) * (w[1] - w[0]) / 2.0;
            worst = worst.max(worst_at(&source.file, (&shipped, &*sa), &fluid, t));
        }
    }
    worst
}

/// The six caloric curves of `shipped` at `t` against `fluid`'s EOS at the densities of its superancillary `sa`, each
/// miss, relative to max(|v|, floor), asserted within `CaloricFit`; returns the largest.
fn worst_at(file: &str, (shipped, sa): (&FluidRecord, &dyn SaturationCurve), fluid: &Fluid, t: f64) -> f64 {
    let side = |phase| FlashOptions::new().with_phase(phase).with_domain(DomainPolicy::Extrapolate);
    let r = shipped.eos.gas_constant;
    let sat = sa.at_t(t).unwrap();
    let at = |rho: f64, phase| {
        let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
        fluid.flash(input, &side(phase)).unwrap()
    };
    let (l, v) = (at(sat.bubble.rho, Phase::Liquid), at(sat.dew.rho, Phase::Gas));
    let m = Basis::Molar;
    let exact = [l.h(m), v.h(m), l.s(m), v.s(m), l.u(m), v.u(m)];
    let curves = shipped.caloric_view().unwrap().at(t).unwrap();
    let mut worst = 0.0_f64;
    for (c, (got, want)) in curves.iter().zip(exact).enumerate() {
        let floor = if c == 2 || c == 3 { r } else { r * t };
        let miss = (got - want).abs() / want.abs().max(floor);
        assert!(miss <= caloric::FIT_TOL, "{file} curve {c} at {t} K: {got} against {want}");
        worst = worst.max(miss);
    }
    worst
}
