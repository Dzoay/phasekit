use phasekit_core::{Basis, DataSet, DomainError, Error, Input, Pair, Phase, Prop, Registry};

use super::*;

/// Core's embedded IAPWS-95 water under `data`, and ice on it.
fn ice_on(data: DataSet) -> Fluid {
    let water = Registry::from_embedded(data).unwrap().get("Water").unwrap().clone();
    ice(&water, data).unwrap()
}

fn pt(p: f64, t: f64) -> Input {
    Input::new(Pair::PT, p, t, Basis::Mass).unwrap()
}

/// R10-06(2009) §5: from 0 K, exclusive, to 273.16 K and to 210 MPa, both ends inside; a step beyond is refused.
#[test]
fn ice_takes_its_range_and_refuses_beyond() {
    let ice = ice_on(DataSet::Corrected);
    for (p, t) in [(1e5, gibbs::T_T), (P_MAX, 200.0), (1e5, 1e-3)] {
        assert_eq!(ice.state(pt(p, t)).map(|s| s.phase()), Ok(Phase::Solid), "({p}, {t})");
    }
    let hot = DomainError::AboveMaxTemperature { t: 273.17, t_max: gibbs::T_T };
    assert_eq!(ice.state(pt(1e5, 273.17)).unwrap_err(), hot.into());
    let over = DomainError::AboveMaxPressure { p: 210.1e6, p_max: P_MAX };
    assert_eq!(ice.state(pt(210.1e6, 200.0)).unwrap_err(), over.into());
}

/// The two data sets are two models (their g₀₀ differs, DIV-0066), each on water's molar mass and gas constant;
/// only PT is declared until M14.5; the limits state 273.16 K and 210 MPa.
#[test]
fn ice_is_a_model_of_its_own_per_data_set() {
    let (corrected, parity) = (ice_on(DataSet::Corrected), ice_on(DataSet::Parity));
    assert_ne!(corrected.model().info().key(), parity.model().info().key());
    let water = Registry::from_embedded(DataSet::Corrected).unwrap().get("Water").unwrap().clone();
    assert_eq!(corrected.model().info().molar_mass(), water.model().info().molar_mass());
    let state = corrected.state(pt(1e5, 250.0)).unwrap();
    assert_eq!(state.gas_constant(), water.model().helmholtz().unwrap().gas_constant());
    assert_eq!(corrected.model().capabilities(), Capabilities::none().with(Pair::PT));
    let limits = corrected.model().limits().unwrap();
    assert_eq!((limits.t_max(), limits.p_max(), limits.t_min() > 0.0), (gibbs::T_T, P_MAX, true));
    let qt = Input::new(Pair::QT, 0.5, 250.0, Basis::Mass).unwrap();
    assert_eq!(corrected.state(qt).unwrap_err(), Error::Unsupported { pair: Pair::QT });
}

/// A water without a Helmholtz model has no gas constant to share: refused.
#[test]
fn ice_needs_a_helmholtz_water() {
    let toy = ice_on(DataSet::Corrected);
    assert_eq!(
        ice(&toy, DataSet::Corrected).unwrap_err(),
        Error::NoModel { prop: Prop::Constant(Constant::GasConstant) }
    );
}
