//! User decision EM15: a remedy core names works when the caller follows it.

use super::*;
use crate::error::Remedy;

/// Water's refusals and the remedy each names, followed: a domain limit under `Extrapolate`, PT's two roots at psat by
/// `Nearest`, a two-phase state's c_p by its saturated phases.
#[test]
fn a_remedy_works_when_followed() {
    let water = embedded("Water");
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    for (p, t) in [(1e6, 3000.0), (1e5, 260.0), (2e9, 400.0)] {
        let refused = water.state(pt(p, t)).unwrap_err();
        assert_eq!(refused.remedies(), [Remedy::Extrapolate], "{refused:?}");
        assert!(water.flash(pt(p, t), &extrapolate).is_ok_and(|s| s.is_extrapolated()), "{p} Pa, {t} K");
    }
    let psat = record_of("Water").superancillary_curve().unwrap().at_t(305.0).unwrap().bubble.p;
    let Err(ambiguous @ Error::Ambiguous { roots }) = water.state(pt(psat, 305.0)) else { panic!() };
    assert_eq!(ambiguous.remedies(), [Remedy::ChooseRoot]);
    for root in roots.as_slice() {
        let chosen = water.flash(pt(psat, 305.0), &FlashOptions::new().with_roots(RootPolicy::Nearest(*root)));
        assert!(chosen.is_ok_and(|s| s.rho(Basis::Molar) == *root), "{root}");
    }
    let undefined = water.state(q_t(0.5, 305.0)).unwrap().cp(Basis::Mass).unwrap_err();
    assert_eq!(undefined.remedies(), [Remedy::SaturatedPhases]);
    assert!([0.0, 1.0].into_iter().all(|q| water.state(q_t(q, 305.0)).unwrap().cp(Basis::Mass).is_ok()));
}
