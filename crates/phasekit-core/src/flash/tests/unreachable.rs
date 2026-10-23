//! A value no state reaches names the bound the solver reached, which side of it the value lies and the pair's other
//! input, all in the caller's basis and gauge (PLAN.md M29.2; user decision EM16).

use super::*;
use crate::error::Limit;
use crate::fluid::ReferenceState;

/// An input of `pair` with the refused value `v` in its place, mass basis.
type Make = fn(f64) -> Result<Input, Error>;

/// A case: the input, the value no state reaches, the limit, the refused variable, and the given input.
type Case = (Make, f64, Limit, Var, (Var, f64));

/// Water's T+X and Q-pairs, mass basis, each value beyond every state at the other input: the input, the value, the
/// limit and the given input.
const CASES: [Case; 10] = [
    (|v| Input::new(Pair::HT, v, 300.0, Basis::Mass), 1e7, Limit::Max, Var::H, (Var::T, 300.0)),
    (|v| Input::new(Pair::HT, v, 300.0, Basis::Mass), -1e7, Limit::Min, Var::H, (Var::T, 300.0)),
    (|v| Input::new(Pair::ST, v, 300.0, Basis::Mass), 1e6, Limit::Max, Var::S, (Var::T, 300.0)),
    (|v| Input::new(Pair::ST, v, 300.0, Basis::Mass), -1e5, Limit::Min, Var::S, (Var::T, 300.0)),
    (|v| Input::new(Pair::TU, 300.0, v, Basis::Mass), 1e7, Limit::Max, Var::U, (Var::T, 300.0)),
    (|v| Input::new(Pair::TU, 300.0, v, Basis::Mass), -1e7, Limit::Min, Var::U, (Var::T, 300.0)),
    (|v| Input::new(Pair::QS, 0.5, v, Basis::Mass), 1e6, Limit::Max, Var::S, (Var::Q, 0.5)),
    (|v| Input::new(Pair::QS, 0.5, v, Basis::Mass), -1e6, Limit::Min, Var::S, (Var::Q, 0.5)),
    (|v| Input::new(Pair::HQ, v, 0.5, Basis::Mass), 1e7, Limit::Max, Var::H, (Var::Q, 0.5)),
    (|v| Input::new(Pair::DQ, v, 0.5, Basis::Mass), 1e6, Limit::Max, Var::D, (Var::Q, 0.5)),
];

/// Each of [`CASES`] on Water at its native gauge and at NBP's: refused as `Unreachable` with the value and the given
/// input as the caller gave them, mass basis; the limit; and a bound the flash reaches, moved inside by 1e-9 of it
/// (one root, or several where the bound is a turn of x). The bound is the equation of state's extreme along the
/// search, which can lie outside the domain (Water's s at 300 K is least at the isotherm's densest end, above p_max), so
/// it is reached under `Extrapolate`; 1e-6 of it past the bound is refused.
#[test]
fn an_unreachable_value_names_its_bound() {
    let native = embedded("Water");
    let nbp = native.with_reference(ReferenceState::Nbp).unwrap();
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    for water in [&native, &nbp] {
        for (make, value, limit, var, (given, at)) in CASES {
            let refused = water.state(make(value).unwrap());
            let Err(Error::Unreachable { quantity, value: v, limit: l, bound, given: g }) = refused else {
                panic!("{value}: {refused:?}")
            };
            let want = (var.quantity(Basis::Mass), value, limit, (given.quantity(Basis::Mass), at));
            assert_eq!((quantity, v, l, g), want, "{value}");
            let inward = if limit == Limit::Max { -bound.abs() } else { bound.abs() };
            let reached = water.flash(make(bound + 1e-9 * inward).unwrap(), &extrapolate);
            assert!(matches!(reached, Ok(_) | Err(Error::Ambiguous { .. })), "{value} inside {bound}: {reached:?}");
            let beyond = water.flash(make(bound - 1e-6 * inward).unwrap(), &extrapolate);
            assert!(
                matches!(beyond, Err(Error::Unreachable { limit: l, .. }) if l == limit),
                "past {bound}: {beyond:?}"
            );
        }
    }
}

/// The message names the value, the given input and the bound in the caller's units (here SI, mass basis).
#[test]
fn an_unreachable_value_in_both_styles() {
    let (make, value, ..) = CASES[0];
    let refused = embedded("Water").state(make(value).unwrap()).unwrap_err();
    assert_eq!(refused.code(), "unreachable");
    let long = "enthalpy 1e7 J/kg is above the highest a state reaches at temperature 300 K, 2.55143e6 J/kg";
    assert_eq!(refused.to_string(), long);
    assert_eq!(format!("{refused:#}"), "h = 1e7 J/kg > h_max(T = 300 K) = 2.55143e6 J/kg");
}
