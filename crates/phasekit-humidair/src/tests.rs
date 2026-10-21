use phasekit_core::{Basis, DataSet, Density, DomainError, Error, Input, Pressure, Registry, Temperature};

use crate::{HumidAir, Humidity, Parts, Per};

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

/// The pressure at which moist air of mole fraction `psi` has molar volume `v` at `t`: the virial equation.
fn pressure(air: &HumidAir, t: f64, v: f64, psi: f64) -> f64 {
    let virials = air.virials(t).unwrap();
    let (x, rt) = (1.0 - psi, crate::constants::R * t);
    let b = x * x * virials.b_aa.value + 2.0 * x * psi * virials.b_aw.value + psi * psi * virials.b_ww.value;
    let c = x * x * x * virials.c_aaa.value
        + 3.0 * x * x * psi * virials.c_aaw.value
        + 3.0 * x * psi * psi * virials.c_aww.value
        + psi * psi * psi * virials.c_www.value;
    rt / v * (1.0 + b / v + c / (v * v))
}

/// ROT-269 (PLAN.md M18.3): c_p, c_v and w are the state's own derivatives, against central differences of h at
/// constant p, of u at constant v̄ and of p at constant T, Richardson-extrapolated, at dry, humid and nearly saturated
/// air from 200 to 600 K and 10 kPa to 9 MPa, under both data sets.
#[test]
fn heat_capacities_and_sound_are_the_states_own_derivatives() {
    use Per::MolHumidAir as Mol;
    let richardson = |f: &dyn Fn(f64) -> f64, h: f64| {
        let d = |h: f64| (f(h) - f(-h)) / (2.0 * h);
        (4.0 * d(h / 2.0) - d(h)) / 3.0
    };
    for data in [DataSet::Corrected, DataSet::Parity] {
        let air = humid_air(data);
        for (t, p, psi) in [(200.0, 1e4, 0.0), (300.0, 101_325.0, 0.02), (350.0, 1e6, 0.03), (600.0, 9e6, 0.4)] {
            let state = air.moist_air(t, p, psi).unwrap();
            let v = state.v(Mol);
            let cp = richardson(&|dt| air.moist_air(t + dt, p, psi).unwrap().h(Mol), 0.05);
            let u_at = |dt: f64| air.moist_air(t + dt, pressure(&air, t + dt, v, psi), psi).unwrap().u(Mol);
            let cv = richardson(&u_at, 0.05);
            let dp_dv = richardson(&|dv| pressure(&air, t, v + dv, psi), v * 1e-4);
            let w = phasekit_core::math::sqrt(-v * v * state.cp(Mol) / state.cv(Mol) * dp_dv / state.molar_mass());
            for (what, got, want) in
                [("cp", state.cp(Mol), cp), ("cv", state.cv(Mol), cv), ("w", state.speed_of_sound(), w)]
            {
                assert!((got / want - 1.0).abs() < 1e-8, "{data:?} {what} at {t} K, {p} Pa, ψ {psi}: {got} {want}");
            }
        }
    }
}

/// Under `Corrected`, dry air's h and s are zero at 273.15 K and 101.325 kPa (PLAN.md M18.3); per kilogram of dry
/// air, of humid air and per mole are one state's, and Z, u and the isentropic exponent follow from it.
#[test]
fn the_state_has_its_zero_and_its_bases() {
    let air = humid_air(DataSet::Corrected);
    let dry = air.moist_air(273.15, 101_325.0, 0.0).unwrap();
    assert!(dry.h(Per::KgDryAir).abs() < 1e-9 && dry.s(Per::KgDryAir).abs() < 1e-12, "{dry:?}");
    let state = air.moist_air(320.0, 2e5, 0.05).unwrap();
    let (m, w) = (state.molar_mass(), state.humidity_ratio());
    for (per_mol, per_kg_ha, per_kg_da) in [
        (state.h(Per::MolHumidAir), state.h(Per::KgHumidAir), state.h(Per::KgDryAir)),
        (state.s(Per::MolHumidAir), state.s(Per::KgHumidAir), state.s(Per::KgDryAir)),
        (state.v(Per::MolHumidAir), state.v(Per::KgHumidAir), state.v(Per::KgDryAir)),
    ] {
        assert!((per_kg_ha * m / per_mol - 1.0).abs() < 1e-15);
        assert!((per_kg_da / ((1.0 + w) * per_kg_ha) - 1.0).abs() < 1e-14);
    }
    let v = state.v(Per::MolHumidAir);
    assert_eq!(state.u(Per::MolHumidAir), state.h(Per::MolHumidAir) - 2e5 * v);
    assert!((state.z() - 2e5 * v / (crate::constants::R * 320.0)).abs() < 1e-15);
    assert_eq!((state.t(), state.p(), state.mole_fraction(), state.partial_pressure()), (320.0, 2e5, 0.05, 1e4));
    let gamma = state.isentropic_exponent();
    let w2 = gamma * 2e5 * v / m;
    assert!((state.speed_of_sound() * state.speed_of_sound() / w2 - 1.0).abs() < 1e-14);
}

/// Where the virial equation has no state: at 40 °C and W = 0.1 its pressure peaks at 7.86 MPa, the end of its stable
/// branch, so 10 MPa is refused with that peak (v8.0.0's secant returns its 100th iterate); just below the peak the
/// state is on the branch.
#[test]
fn a_pressure_past_the_virial_branch_is_refused() {
    let air = humid_air(DataSet::Corrected);
    let psi = air.mole_fraction(313.15, 1e7, Humidity::Ratio(0.1)).unwrap();
    let refused = air.moist_air(313.15, 1e7, psi);
    let Err(Error::Domain(DomainError::AboveMaxPressure { p, p_max })) = refused else { panic!("{refused:?}") };
    assert!(p == 1e7 && (p_max / 7.86e6 - 1.0).abs() < 1e-3, "{p}: {p_max}");
    let below = air.moist_air(313.15, 0.99 * p_max, psi).unwrap();
    assert!(below.z() > 0.0 && below.cp(Per::MolHumidAir) > 0.0, "{below:?}");
}

/// v8.0.0's range (130 to 623.15 K, 10 Pa to 10 MPa) and ψ_w in [0, 1) bound a state, each refusal typed.
#[test]
fn the_state_refuses_outside_its_range() {
    let air = humid_air(DataSet::Corrected);
    let refused = |t: f64, p: f64, psi: f64| air.moist_air(t, p, psi).unwrap_err();
    assert_eq!(refused(129.0, 1e5, 0.0), DomainError::BelowMinTemperature { t: 129.0, t_min: 130.0 }.into());
    assert_eq!(refused(624.0, 1e5, 0.0), DomainError::AboveMaxTemperature { t: 624.0, t_max: 623.15 }.into());
    assert_eq!(refused(300.0, 9.0, 0.0), DomainError::BelowMinPressure { p: 9.0, p_min: 10.0 }.into());
    assert_eq!(refused(300.0, 2e7, 0.0), DomainError::AboveMaxPressure { p: 2e7, p_max: 1e7 }.into());
    assert!(matches!(refused(f64::NAN, 1e5, 0.0), Error::Domain(DomainError::BelowMinTemperature { .. })));
    assert!(matches!(refused(300.0, f64::NAN, 0.0), Error::Domain(DomainError::BelowMinPressure { .. })));
    for psi in [-0.1, 1.0, f64::NAN] {
        assert!(matches!(refused(300.0, 1e5, psi), Error::InvalidInput { quantity: "mole fraction of water", .. }));
    }
    assert!(air.moist_air(130.0, 10.0, 0.0).is_ok() && air.moist_air(623.15, 1e7, 0.0).is_ok());
}

/// Where water's saturation pressure is p or above (RP-1485 Eq. (3.25)'s note), no condensed phase is under pressure:
/// its κ_T and Henry's constant are 0, over ice (250 K) and liquid (300 K), at p_ws itself too; above it they are its
/// part's.
#[test]
fn the_condensed_phase_is_unpressed_at_and_below_saturation() {
    let air = humid_air(DataSet::Corrected);
    for t in [250.0, 300.0] {
        let p_ws = air.saturation(t, 1e5).unwrap().p_ws;
        for p in [p_ws / 2.0, p_ws] {
            let s = air.saturation(t, p).unwrap();
            assert_eq!((s.kappa_t, s.beta_h), (0.0, 0.0), "{t} K, {p} Pa");
        }
        assert!(air.saturation(t, 2.0 * p_ws).unwrap().kappa_t > 0.0, "{t} K");
    }
}

/// Under `Parity` the liquid's κ_T is v8.0.0's: IAPWS-95's at IF97's liquid density, bit for bit, not IF97's own.
#[test]
fn parity_takes_iapws_95s_compressibility_at_if97s_density() {
    let air = humid_air(DataSet::Parity);
    let (t, p) = (Temperature::new(300.0).unwrap(), Pressure::new(1e5).unwrap());
    let liquid = air.parts().liquid.state(Input::pt(p, t)).unwrap();
    let density = Density::molar(liquid.rho(Basis::Molar)).unwrap();
    let want = air.parts().water.state(Input::dt(density, t)).unwrap().isothermal_compressibility().unwrap();
    let got = air.saturation(300.0, 1e5).unwrap().kappa_t;
    assert_eq!(got.to_bits(), want.to_bits());
    assert_ne!(got, liquid.isothermal_compressibility().unwrap());
}

/// Eq. (3.25)'s f settles across the range, both data sets: at 130 to 600 K and 10 Pa to 10 MPa, up to about 192 at
/// 130 K and 10 MPa, at least 1 everywhere.
#[test]
fn the_enhancement_factor_settles_across_the_range() {
    for data in [DataSet::Corrected, DataSet::Parity] {
        let air = humid_air(data);
        let mut largest: f64 = 0.0;
        for t in [130.0, 200.0, 250.0, 273.16, 300.0, 350.0, 450.0, 600.0] {
            for p in [10.0, 1e3, 1e5, 1e6, 1e7] {
                let f = air.saturation(t, p).unwrap().f;
                assert!(f >= 1.0, "{data:?} {t} K, {p} Pa: {f}");
                largest = largest.max(f);
            }
        }
        assert!((191.0..193.0).contains(&largest), "{data:?}: {largest}");
    }
}

/// Under `Parity`, water vapour's ideal part sits on v8.0.0's anchor (`HumidAirProp.cpp:281-326` and `:1083`, map 15
/// §2.1): at 473.15 K and 101.325 kPa its h is 51885.582451893446 J/mol plus the legacy −0.01102303806, and its s
/// 141.18297895840303 J/(mol K) plus s̄₀, 0.02366427495.
#[test]
fn parity_s_vapour_sits_on_v8s_anchor() {
    let air = humid_air(DataSet::Parity);
    let part = crate::ideal::water(air.parts(), DataSet::Parity, &air.zeros, 473.15, 101_325.0).unwrap();
    assert!((part.h - (51_885.582_451_893_446 - 0.011_023_038_06)).abs() <= 1e-9, "{}", part.h);
    assert!((part.s - (141.182_978_958_403_03 + 0.023_664_274_95)).abs() <= 1e-12, "{}", part.s);
}

/// Under `Corrected`, water vapour's ideal part is IF97's from the triple point up, at 273.16 K itself too, and
/// IAPWS-95's below it.
#[test]
fn corrected_s_vapour_is_if97_s_from_the_triple_point() {
    let air = humid_air(DataSet::Corrected);
    let h = |t: f64| crate::ideal::water(air.parts(), DataSet::Corrected, &air.zeros, t, 101_325.0).unwrap().h;
    let if97 = |t: f64| crate::ideal::water_if97(&air.parts().liquid, t, 101_325.0).unwrap().h + air.zeros.h_water;
    assert_eq!(h(273.16).to_bits(), if97(273.16).to_bits());
    assert_ne!(h(273.159), if97(273.159));
}
