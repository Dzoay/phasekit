use phasekit_core::{DataSet, DomainError, Error, Registry};

use crate::{HumidAir, Humidity, Parts};

/// Humid air on core's water and Air and phasekit-iapws's IF97 water and ice, under `data`.
fn humid_air(data: DataSet) -> HumidAir {
    let registry = Registry::from_embedded(data).unwrap();
    let water = registry.get("Water").unwrap().clone();
    let parts = Parts {
        air: registry.get("Air").unwrap().clone(),
        liquid: phasekit_iapws::if97::water().unwrap(),
        ice: phasekit_iapws::ice::ice(&water, data).unwrap(),
        water,
    };
    HumidAir::new(parts, data).unwrap()
}

/// W, ψ_w, φ and the dew point name one state: each round-trips through ψ_w at 300 K and 1 atm, φ = 1 and a dew point
/// at T are saturation, and below the triple point the dew point is a frost point, on ice's sublimation curve.
#[test]
fn the_humidity_measures_name_one_state() {
    let air = humid_air(DataSet::Corrected);
    let (t, p) = (300.0, 101_325.0);
    let psi = air.mole_fraction(t, p, Humidity::Ratio(0.01)).unwrap();
    assert!((air.humidity_ratio(psi) / 0.01 - 1.0).abs() < 1e-15);
    assert_eq!(air.mole_fraction(t, p, Humidity::MoleFraction(psi)).unwrap(), psi);
    let saturated = air.saturation(t, p).unwrap().psi_ws;
    assert_eq!(air.mole_fraction(t, p, Humidity::Relative(1.0)).unwrap(), saturated);
    assert_eq!(air.mole_fraction(t, p, Humidity::DewPoint(t)).unwrap(), saturated);
    let phi = air.relative_humidity(t, p, air.mole_fraction(t, p, Humidity::Relative(0.4)).unwrap()).unwrap();
    assert!((phi - 0.4).abs() < 1e-15, "{phi}");
    let frost = air.mole_fraction(t, p, Humidity::DewPoint(260.0)).unwrap();
    let ice = phasekit_iapws::ice::sublimation_pressure(260.0).unwrap();
    assert_eq!(frost, air.saturation(260.0, p).unwrap().f * ice / p);
    assert!((air.saturated_humidity_ratio(t, p).unwrap() - air.humidity_ratio(saturated)).abs() < 1e-18);
}

/// A negative W, ψ_w at or above 1, φ outside [0, 1] and a dew point warmer than T are refused, typed.
#[test]
fn impossible_humidities_are_refused() {
    let air = humid_air(DataSet::Corrected);
    let refused = |h: Humidity| air.mole_fraction(300.0, 101_325.0, h).unwrap_err();
    assert!(matches!(refused(Humidity::Ratio(-1e-3)), Error::InvalidInput { quantity: "humidity ratio", .. }));
    assert!(matches!(refused(Humidity::MoleFraction(1.0)), Error::InvalidInput { .. }));
    assert!(matches!(refused(Humidity::Relative(1.01)), Error::InvalidInput { quantity: "relative humidity", .. }));
    assert!(matches!(refused(Humidity::Relative(-0.1)), Error::InvalidInput { .. }));
    let warm = DomainError::AboveMaxTemperature { t: 301.0, t_max: 300.0 };
    assert_eq!(refused(Humidity::DewPoint(301.0)), warm.into());
    assert_eq!(air.mole_fraction(300.0, 101_325.0, Humidity::Ratio(0.0)).unwrap(), 0.0);
}

/// Henry's constants refuse the supercritical solvent, where G7-04's τ^0.355 has no value.
#[test]
fn henry_stops_at_the_critical_point() {
    let above = DomainError::AboveMaxTemperature { t: 650.0, t_max: 647.096 };
    assert_eq!(crate::henry_constant(crate::Gas::N2, 650.0).unwrap_err(), above.into());
    assert!(crate::henry_constant(crate::Gas::O2, 647.096).is_ok());
}
