//! Every message in both styles (PLAN.md M28.1; issue #191; user decisions EM1-EM6).

use super::*;
use crate::prop::{Constant, DerivVar, Partial};
use crate::quantity::{Dimension, Unit};
use crate::units::Basis;

fn long(e: &Error) -> String {
    e.to_string()
}

fn short(e: &Error) -> String {
    format!("{e:#}")
}

fn invalid(quantity: &'static Quantity, value: f64) -> Error {
    Error::InvalidInput { quantity, value }
}

/// Water's h at 300 K above every state's.
fn unreachable() -> Error {
    let given = (Quantity::TEMPERATURE, 300.0);
    Error::Unreachable { quantity: Quantity::enthalpy(Basis::Mass), value: 1e7, limit: Limit::Max, bound: 2.5e6, given }
}

/// The engineer's units of the site's second system: °C, bar, kJ.
struct Engineering;

impl Units for Engineering {
    fn unit(&self, dimension: Dimension) -> Unit<'_> {
        match dimension {
            Dimension::Temperature => Unit { label: "°C", scale: 1.0, offset: 273.15 },
            Dimension::Pressure => Unit { label: "bar", scale: 1e5, offset: 0.0 },
            Dimension::Energy(Basis::Mass) => Unit { label: "kJ/kg", scale: 1e3, offset: 0.0 },
            other => Si.unit(other),
        }
    }
}

/// Issue #191's six messages, and each rule an invalid value breaks, in SI and both styles.
#[test]
fn every_message_in_both_styles() {
    let cases = [
        (invalid(Quantity::TEMPERATURE, 0.0), "temperature 0 K is not above absolute zero", "T = 0 K ≤ 0 K"),
        (invalid(Quantity::PRESSURE, -5.0), "pressure −5 Pa is not positive", "p = −5 Pa ≤ 0 Pa"),
        (invalid(Quantity::QUALITY, 1.5), "quality 1.5 is outside [0, 1]", "Q = 1.5 ∉ [0, 1]"),
        (invalid(Quantity::TEMPERATURE, f64::NAN), "temperature is not a number", "T = NaN"),
        (invalid(Quantity::density(Basis::Mass), f64::INFINITY), "density ∞ kg/m³ is not finite", "ρ = ∞ kg/m³"),
        (
            invalid(Quantity::enthalpy(Basis::Molar), 1e7),
            "enthalpy 1e7 J/mol is out of range",
            "h = 1e7 J/mol: out of range",
        ),
        (
            DomainError::BelowMinTemperature { t: 200.0, t_min: 273.16 }.into(),
            "temperature 200 K is below the minimum, 273.16 K",
            "T = 200 K < T_min = 273.16 K",
        ),
        (
            DomainError::AboveMaxTemperature { t: 700.0, t_max: 647.0959999999873 }.into(),
            "temperature 700 K is above the maximum, 647.096 K",
            "T = 700 K > T_max = 647.096 K",
        ),
        (
            DomainError::BelowMinPressure { p: 100.0, p_min: 611.655 }.into(),
            "pressure 100 Pa is below the minimum, 611.655 Pa",
            "p = 100 Pa < p_min = 611.655 Pa",
        ),
        (
            DomainError::AboveMaxPressure { p: 2e9, p_max: 1e9 }.into(),
            "pressure 2e9 Pa is above the maximum, 1e9 Pa",
            "p = 2e9 Pa > p_max = 1e9 Pa",
        ),
        (
            DomainError::BelowMeltingLine { t: 250.0, t_melt: 251.165 }.into(),
            "temperature 250 K is below the melting temperature at this pressure, 251.165 K",
            "T = 250 K < T_melt = 251.165 K",
        ),
        (
            DomainError::MechanicallyUnstable.into(),
            "the state is mechanically unstable: dp/dρ ≤ 0 or c_v ≤ 0",
            "unstable: dp/dρ ≤ 0 or c_v ≤ 0",
        ),
        (
            Error::Ambiguous { roots: Roots::new(Quantity::TEMPERATURE, &[300.0, 350.0]) },
            "several states fit these inputs: T = 300 K or 350 K",
            "T ∈ {300 K, 350 K}",
        ),
        (
            Error::Ambiguous { roots: Roots::new(Quantity::PRESSURE, &[1e5, 2e5, 3e5]) },
            "several states fit these inputs: p = 100000 Pa, 200000 Pa or 300000 Pa",
            "p ∈ {100000 Pa, 200000 Pa, 300000 Pa}",
        ),
        (
            Error::Undefined { prop: Prop::Cpmass, phase: Phase::TwoPhase },
            "the isobaric heat capacity (Cpmass) is undefined for a two-phase state",
            "Cpmass: undefined, two-phase",
        ),
        (Error::NoModel { prop: Prop::Viscosity }, "no model for the viscosity (Viscosity)", "Viscosity: no model"),
        (Error::Unsupported { pair: Pair::HS }, "input pair HS is not supported by this model", "HS: unsupported"),
        (
            Error::NoConvergence { strategy: Strategy::DensityNewton, iterations: 50 },
            "the density Newton iteration (DensityNewton) did not converge in 50 iterations",
            "DensityNewton: no convergence in 50 iterations",
        ),
        (Error::ForeignState, "the state was produced by a different model", "foreign state"),
        (Error::Shape { expected: 3, found: 2 }, "buffer length 2, expected 3", "length 2 ≠ 3"),
        (
            LoadError::UnknownName("Wtaer".into()).into(),
            "load error: unknown fluid name `Wtaer`",
            "unknown fluid name `Wtaer`",
        ),
    ];
    for (error, want_long, want_short) in cases {
        assert_eq!((long(&error), short(&error)), (want_long.to_string(), want_short.to_string()), "{error:?}");
    }
}

/// User decision EM1: each number in the caller's units; a temperature's zero limit too, so the long form says
/// "absolute zero", not "positive".
#[test]
fn messages_take_the_callers_units() {
    let cases = [
        (
            invalid(Quantity::TEMPERATURE, 0.0),
            "temperature −273.15 °C is not above absolute zero",
            "T = −273.15 °C ≤ −273.15 °C",
        ),
        (
            DomainError::AboveMaxPressure { p: 2e9, p_max: 1e9 }.into(),
            "pressure 20000 bar is above the maximum, 10000 bar",
            "p = 20000 bar > p_max = 10000 bar",
        ),
        (
            invalid(Quantity::enthalpy(Basis::Mass), 1e7),
            "enthalpy 10000 kJ/kg is out of range",
            "h = 10000 kJ/kg: out of range",
        ),
        (
            Error::Unreachable {
                quantity: Quantity::enthalpy(Basis::Mass),
                value: 1e7,
                limit: Limit::Max,
                bound: 2.5493e6,
                given: (Quantity::TEMPERATURE, 300.0),
            },
            "enthalpy 10000 kJ/kg is above the highest a state reaches at temperature 26.85 °C, 2549.3 kJ/kg",
            "h = 10000 kJ/kg > h_max(T = 26.85 °C) = 2549.3 kJ/kg",
        ),
        (
            Error::Unreachable {
                quantity: Quantity::entropy(Basis::Mass),
                value: -1e5,
                limit: Limit::Min,
                bound: -0.5,
                given: (Quantity::QUALITY, 0.5),
            },
            "entropy −100000 J/(kg·K) is below the lowest a state reaches at quality 0.5, −0.5 J/(kg·K)",
            "s = −100000 J/(kg·K) < s_min(Q = 0.5) = −0.5 J/(kg·K)",
        ),
        (
            Error::Ambiguous { roots: Roots::new(Quantity::TEMPERATURE, &[273.15, 373.15]) },
            "several states fit these inputs: T = 0 °C or 100 °C",
            "T ∈ {0 °C, 100 °C}",
        ),
    ];
    for (error, want_long, want_short) in cases {
        let written = (
            error.display(&Engineering, Style::Long).to_string(),
            error.display(&Engineering, Style::Short).to_string(),
        );
        assert_eq!(written, (want_long.to_string(), want_short.to_string()), "{error:?}");
    }
}

/// `{}` is the long form in SI, `{:#}` the short one.
#[test]
fn the_alternate_flag_is_the_short_form() {
    let e = invalid(Quantity::QUALITY, 1.5);
    assert_eq!(format!("{e}"), e.display(&Si, Style::Long).to_string());
    assert_eq!(format!("{e:#}"), e.display(&Si, Style::Short).to_string());
}

/// A rule is named only when the value breaks it: a pressure above zero is not "not positive", and an enthalpy, which
/// may be negative, is not either.
#[test]
fn a_rule_the_value_keeps_is_never_named() {
    let cases = [
        (invalid(Quantity::PRESSURE, 2e9), "pressure 2e9 Pa is out of range", "p = 2e9 Pa: out of range"),
        (
            invalid(Quantity::enthalpy(Basis::Mass), -5.0),
            "enthalpy −5 J/kg is out of range",
            "h = −5 J/kg: out of range",
        ),
    ];
    for (error, want_long, want_short) in cases {
        assert_eq!((long(&error), short(&error)), (want_long.to_string(), want_short.to_string()), "{error:?}");
    }
}

/// User decision EM3: a count or an index is an argument, written without a unit.
#[test]
fn structural_arguments_carry_no_unit() {
    let count = Error::InvalidArgument { argument: "number of mole fractions", value: Some(3) };
    assert_eq!(
        (long(&count), short(&count)),
        ("invalid number of mole fractions: 3".into(), "invalid number of mole fractions: 3".into())
    );
    let plane = Error::InvalidArgument { argument: "plane", value: None };
    assert_eq!((long(&plane), short(&plane)), ("invalid plane".into(), "invalid plane".into()));
}

/// A partial derivative is written as `∂of/∂wrt|at`, in the long form after "the derivative"; a constant in words and by
/// its name.
#[test]
fn derivatives_and_constants_by_name() {
    let partial = Partial { of: DerivVar::Hmass, wrt: DerivVar::T, at: DerivVar::P };
    let undefined = Error::Undefined { prop: Prop::Partial(partial), phase: Phase::CriticalPoint };
    assert_eq!(long(&undefined), "the derivative ∂Hmass/∂T|P is undefined for a critical-point state");
    assert_eq!(short(&undefined), "∂Hmass/∂T|P: undefined, critical-point");
    assert_eq!(
        long(&Error::NoModel { prop: Prop::Constant(Constant::TCritical) }),
        "no model for the critical temperature (TCritical)"
    );
}

/// Roots keep their quantity: the same values of another quantity differ, and the unused slots never show.
#[test]
fn roots_carry_their_quantity() {
    let t = Roots::new(Quantity::TEMPERATURE, &[260.0, 280.0]);
    assert_eq!(t.as_slice(), &[260.0, 280.0]);
    assert_eq!(t.quantity(), Quantity::TEMPERATURE);
    assert_ne!(t, Roots::new(Quantity::PRESSURE, &[260.0, 280.0]));
    assert_eq!(Roots::new(Quantity::PRESSURE, &[1.0, 2.0, 3.0, 4.0, 5.0]).as_slice().len(), 4);
}

/// User decision EM11: each error's stable code, a domain error's its own; the browser package's codes as they were.
#[test]
fn every_error_has_its_code() {
    let cases: [(Error, &str); 19] = [
        (invalid(Quantity::TEMPERATURE, 0.0), "invalid_input"),
        (unreachable(), "unreachable"),
        (Error::InvalidArgument { argument: "plane", value: None }, "invalid_argument"),
        (Error::Unsupported { pair: Pair::HS }, "unsupported"),
        (DomainError::BelowMinTemperature { t: 200.0, t_min: 273.16 }.into(), "below_min_temperature"),
        (DomainError::AboveMaxTemperature { t: 700.0, t_max: 647.096 }.into(), "above_max_temperature"),
        (DomainError::BelowMinPressure { p: 100.0, p_min: 611.655 }.into(), "below_min_pressure"),
        (DomainError::AboveMaxPressure { p: 2e9, p_max: 1e9 }.into(), "above_max_pressure"),
        (DomainError::BelowMeltingLine { t: 250.0, t_melt: 251.165 }.into(), "below_melting_line"),
        (DomainError::MechanicallyUnstable.into(), "mechanically_unstable"),
        (Error::NoConvergence { strategy: Strategy::Vle, iterations: 60 }, "no_convergence"),
        (Error::Ambiguous { roots: Roots::new(Quantity::TEMPERATURE, &[300.0, 350.0]) }, "ambiguous"),
        (Error::Undefined { prop: Prop::Cpmass, phase: Phase::TwoPhase }, "undefined"),
        (Error::NoModel { prop: Prop::Viscosity }, "no_model"),
        (Error::ForeignState, "foreign_state"),
        (Error::InvalidState { reason: "a reason" }, "invalid_state"),
        (LoadError::UnknownName("Wtaer".into()).into(), "load"),
        (Error::Shape { expected: 3, found: 2 }, "shape"),
        (Error::InvalidArgument { argument: "number of mole fractions", value: Some(3) }, "invalid_argument"),
    ];
    for (error, code) in cases {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

/// User decision EM11: the first rule an invalid value breaks, and its code; no rule for any other error.
#[test]
fn the_rule_a_value_breaks() {
    let cases = [
        (invalid(Quantity::TEMPERATURE, f64::NAN), Rule::NotANumber, "not_a_number"),
        (invalid(Quantity::TEMPERATURE, f64::INFINITY), Rule::NotFinite, "not_finite"),
        (invalid(Quantity::TEMPERATURE, 0.0), Rule::NotAboveAbsoluteZero, "not_above_absolute_zero"),
        (invalid(Quantity::PRESSURE, -5.0), Rule::NotPositive, "not_positive"),
        (invalid(Quantity::QUALITY, 1.5), Rule::OutsideUnitInterval, "outside_unit_interval"),
        (invalid(Quantity::PRESSURE, 2e9), Rule::OutOfRange, "out_of_range"),
    ];
    for (error, rule, code) in cases {
        assert_eq!(error.rule(), Some(rule), "{error:?}");
        assert_eq!(rule.code(), code);
    }
    assert_eq!(Error::from(DomainError::MechanicallyUnstable).rule(), None);
}

/// User decision EM15: what a caller can do about each error, by its stable code, most direct first.
#[test]
fn every_error_has_its_remedies() {
    let cases: [(Error, &[&str]); 21] = [
        (invalid(Quantity::TEMPERATURE, 0.0), &[]),
        (unreachable(), &["other_pair"]),
        (Error::InvalidArgument { argument: "plane", value: None }, &[]),
        (Error::Unsupported { pair: Pair::HS }, &["declared_pair"]),
        (DomainError::BelowMinTemperature { t: 200.0, t_min: 273.16 }.into(), &["extrapolate"]),
        (DomainError::AboveMaxTemperature { t: 700.0, t_max: 647.096 }.into(), &["extrapolate"]),
        (DomainError::BelowMinPressure { p: 100.0, p_min: 611.655 }.into(), &["extrapolate"]),
        (DomainError::AboveMaxPressure { p: 2e9, p_max: 1e9 }.into(), &["extrapolate"]),
        (DomainError::BelowMeltingLine { t: 250.0, t_melt: 251.165 }.into(), &["extrapolate"]),
        (DomainError::MechanicallyUnstable.into(), &[]),
        (Error::NoConvergence { strategy: Strategy::Vle, iterations: 60 }, &["warm_start", "other_pair"]),
        (Error::Ambiguous { roots: Roots::new(Quantity::TEMPERATURE, &[300.0, 350.0]) }, &["choose_root"]),
        (Error::Undefined { prop: Prop::Cpmass, phase: Phase::TwoPhase }, &["saturated_phases"]),
        (Error::Undefined { prop: Prop::Q, phase: Phase::Liquid }, &[]),
        (Error::NoModel { prop: Prop::Viscosity }, &[]),
        (Error::ForeignState, &[]),
        (Error::InvalidState { reason: "a reason" }, &[]),
        (LoadError::UnknownName("Wtaer".into()).into(), &["nearest_name"]),
        (LoadError::MissingPart("transport").into(), &[]),
        (Error::Shape { expected: 3, found: 2 }, &[]),
        (Error::InvalidArgument { argument: "number of mole fractions", value: Some(3) }, &[]),
    ];
    for (error, codes) in cases {
        let got: Vec<_> = error.remedies().iter().map(|r| r.code()).collect();
        assert_eq!(got, codes, "{error:?}");
    }
}
