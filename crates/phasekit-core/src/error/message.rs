//! An [`Error`]'s message in a caller's units and style (issue #191; user decisions EM1-EM6). Each number is written
//! in the unit of its quantity's dimension, to six significant figures; the error's fields keep the exact value.

use core::fmt;

use super::{Argument, DomainError, Error, Instability, Limit, Roots, Rule, words};
use crate::prop::{Partial, Prop};
use crate::quantity::{InUnit, Quantity, Style, Units};
use crate::state::Phase;

/// An error written in a caller's units and style ([`Error::display`]).
pub struct Message<'a> {
    error: &'a Error,
    units: &'a dyn Units,
    style: Style,
    subject: Option<&'a str>,
}

impl<'a> Message<'a> {
    pub(super) fn new(error: &'a Error, units: &'a dyn Units, style: Style) -> Self {
        Self { error, units, style, subject: None }
    }

    /// The message with a domain limit written as `subject`'s (`below Water's minimum, 273.16 K`) in the long form,
    /// which a facade passes the fluid's name to (PLAN.md M29.5); the short form is unchanged.
    pub fn of(mut self, subject: &'a str) -> Self {
        self.subject = Some(subject);
        self
    }
}

impl fmt::Display for Message<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = Out { f, units: self.units, short: self.style == Style::Short, subject: self.subject };
        match self.error {
            Error::InvalidInput { quantity, value } => out.invalid_input(quantity, *value),
            Error::Unreachable { quantity, value, limit, bound, given } => {
                out.unreachable(quantity, (*value, *bound), *limit, *given)
            }
            Error::InvalidArgument { argument, value } => out.invalid_argument(argument, *value),
            // The site reads a mixture's liquid-liquid refusal by this long form (`site/src/lib/mixtures.ts`, user
            // decision MX4): it stays as it was.
            Error::Unsupported { pair } => out.either(
                format_args!("input pair {pair:?} is not supported by this model"),
                format_args!("{pair:?}: unsupported"),
            ),
            Error::Domain(e) => out.domain(e),
            Error::NoConvergence { strategy, iterations, .. } => out.either(
                format_args!(
                    "the {} ({strategy:?}) did not converge in {iterations} iterations",
                    words::strategy(*strategy)
                ),
                format_args!("{strategy:?}: no convergence in {iterations} iterations"),
            ),
            Error::Ambiguous { roots } => out.ambiguous(roots),
            Error::Undefined { prop, phase } => out.undefined(*prop, *phase),
            Error::NoModel { prop } => {
                let (words, key) = (PropWords(*prop), PropName(*prop));
                out.either(format_args!("no model for {words}"), format_args!("{key}: no model"))
            }
            Error::ForeignState => {
                out.either(format_args!("the state was produced by a different model"), format_args!("foreign state"))
            }
            Error::InvalidState { reason } => out.f.write_fmt(format_args!("inconsistent state: {reason}")),
            Error::Load(e) => out.either(format_args!("load error: {e}"), format_args!("{e}")),
            Error::Shape { buffer, expected, found } => out.either(
                format_args!("the {buffer} buffer has length {found}, expected {expected}"),
                format_args!("{buffer}: length {found} ≠ {expected}"),
            ),
        }
    }
}

/// Where a message is written, in which units and style.
struct Out<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    units: &'a dyn Units,
    short: bool,
    /// Whose limits a domain error names, if the caller said.
    subject: Option<&'a str>,
}

/// A limit a value is beyond: `temperature 200 K is below the minimum, 273.16 K`, `T = 200 K < T_min = 273.16 K`.
struct Bound {
    quantity: &'static Quantity,
    value: f64,
    limit: f64,
    above: bool,
    /// The limit as a sentence names it ("minimum").
    name: &'static str,
    /// The limit's subscript in the short form ("min").
    subscript: &'static str,
    /// The input the limit is taken at, if it depends on one (the melting temperature's pressure).
    at: Option<(&'static Quantity, f64)>,
}

impl<'a> Out<'a, '_> {
    /// The long or the short form.
    fn either(&mut self, long: fmt::Arguments<'_>, short: fmt::Arguments<'_>) -> fmt::Result {
        self.f.write_fmt(if self.short { short } else { long })
    }

    /// `value` of `quantity`'s dimension in the caller's unit.
    fn value(&self, quantity: &Quantity, value: f64) -> InUnit<'a> {
        InUnit(value, self.units.unit(quantity.dimension()))
    }

    fn invalid_input(&mut self, quantity: &Quantity, value: f64) -> fmt::Result {
        let rule = Rule::of(quantity, value);
        let (v, zero, one) = (self.value(quantity, value), self.value(quantity, 0.0), self.value(quantity, 1.0));
        if self.short {
            let symbol = quantity.symbol();
            match rule {
                Rule::NotANumber => write!(self.f, "{symbol} = NaN"),
                Rule::NotFinite => write!(self.f, "{symbol} = {v}"),
                Rule::NotAboveAbsoluteZero | Rule::NotPositive => write!(self.f, "{symbol} = {v} ≤ {zero}"),
                Rule::OutsideUnitInterval => write!(self.f, "{symbol} = {v} ∉ [{zero}, {one}]"),
                Rule::OutOfRange => write!(self.f, "{symbol} = {v}: out of range"),
            }
        } else {
            let name = quantity.name();
            match rule {
                Rule::NotANumber => write!(self.f, "{name} is not a number"),
                Rule::NotFinite => write!(self.f, "{name} {v} is not finite"),
                Rule::NotAboveAbsoluteZero => write!(self.f, "{name} {v} is not above absolute zero"),
                Rule::NotPositive => write!(self.f, "{name} {v} is not positive"),
                Rule::OutsideUnitInterval => write!(self.f, "{name} {v} is outside [{zero}, {one}]"),
                Rule::OutOfRange => write!(self.f, "{name} {v} is out of range"),
            }
        }
    }

    /// `enthalpy 1e7 J/kg is above the highest a state reaches at temperature 300 K, 2.54932e6 J/kg`,
    /// `h = 1e7 J/kg > h_max(T = 300 K) = 2.54932e6 J/kg` (user decision EM16).
    fn unreachable(
        &mut self,
        quantity: &Quantity,
        (value, bound): (f64, f64),
        limit: Limit,
        (given, at): (&Quantity, f64),
    ) -> fmt::Result {
        let (v, b, at) = (self.value(quantity, value), self.value(quantity, bound), self.value(given, at));
        let (name, symbol, given_name, given_symbol) =
            (quantity.name(), quantity.symbol(), given.name(), given.symbol());
        let (side, extreme, relation, subscript) = match limit {
            Limit::Max => ("above", "highest", ">", "max"),
            Limit::Min => ("below", "lowest", "<", "min"),
        };
        self.either(
            format_args!("{name} {v} is {side} the {extreme} a state reaches at {given_name} {at}, {b}"),
            format_args!("{symbol} = {v} {relation} {symbol}_{subscript}({given_symbol} = {at}) = {b}"),
        )
    }

    /// `number of mole fractions 3 is invalid: expected one for each component`, `number of mole fractions = 3:
    /// expected one for each component` (PLAN.md M29.5).
    fn invalid_argument(&mut self, argument: &Argument, value: Option<usize>) -> fmt::Result {
        let (name, expected) = (argument.name(), argument.expected());
        match value {
            Some(n) => self.either(
                format_args!("{name} {n} is invalid: expected {expected}"),
                format_args!("{name} = {n}: expected {expected}"),
            ),
            None => self.either(
                format_args!("{name} is invalid: expected {expected}"),
                format_args!("{name}: expected {expected}"),
            ),
        }
    }

    fn domain(&mut self, e: &DomainError) -> fmt::Result {
        let (t, p) = (Quantity::TEMPERATURE, Quantity::PRESSURE);
        let bound = match *e {
            DomainError::BelowMinTemperature { t: value, t_min } => {
                Bound { quantity: t, value, limit: t_min, above: false, name: "minimum", subscript: "min", at: None }
            }
            DomainError::AboveMaxTemperature { t: value, t_max } => {
                Bound { quantity: t, value, limit: t_max, above: true, name: "maximum", subscript: "max", at: None }
            }
            DomainError::BelowMinPressure { p: value, p_min } => {
                Bound { quantity: p, value, limit: p_min, above: false, name: "minimum", subscript: "min", at: None }
            }
            DomainError::AboveMaxPressure { p: value, p_max } => {
                Bound { quantity: p, value, limit: p_max, above: true, name: "maximum", subscript: "max", at: None }
            }
            DomainError::BelowMeltingLine { t: value, t_melt, p: at } => Bound {
                quantity: t,
                value,
                limit: t_melt,
                above: false,
                name: "melting temperature",
                subscript: "melt",
                at: Some((p, at)),
            },
            DomainError::MechanicallyUnstable { condition } => {
                let condition = instability(condition);
                return self.either(
                    format_args!("the state is mechanically unstable: {condition}"),
                    format_args!("unstable: {condition}"),
                );
            }
        };
        self.bound(&bound)
    }

    /// `temperature 250 K is below Water's melting temperature at 2e8 Pa, 251.2 K`,
    /// `T = 250 K < T_melt(p = 2e8 Pa) = 251.2 K`; "the" where no subject is given.
    fn bound(&mut self, b: &Bound) -> fmt::Result {
        let (name, symbol) = (b.quantity.name(), b.quantity.symbol());
        let (v, limit) = (self.value(b.quantity, b.value), self.value(b.quantity, b.limit));
        let (side, relation) = if b.above { ("above", ">") } else { ("below", "<") };
        let (what, subscript) = (b.name, b.subscript);
        let whose = Whose(self.subject);
        match b.at {
            Some((quantity, at)) => {
                let (at_symbol, at) = (quantity.symbol(), self.value(quantity, at));
                self.either(
                    format_args!("{name} {v} is {side} {whose} {what} at {at}, {limit}"),
                    format_args!("{symbol} = {v} {relation} {symbol}_{subscript}({at_symbol} = {at}) = {limit}"),
                )
            }
            None => self.either(
                format_args!("{name} {v} is {side} {whose} {what}, {limit}"),
                format_args!("{symbol} = {v} {relation} {symbol}_{subscript} = {limit}"),
            ),
        }
    }

    fn ambiguous(&mut self, roots: &Roots) -> fmt::Result {
        let quantity = roots.quantity();
        let values: Vec<String> = roots.as_slice().iter().map(|&r| self.value(quantity, r).to_string()).collect();
        let symbol = quantity.symbol();
        match values.split_last() {
            None => self.either(format_args!("several states fit these inputs"), format_args!("ambiguous")),
            Some((last, [])) => self.either(
                format_args!("several states fit these inputs: {symbol} = {last}"),
                format_args!("{symbol} ∈ {{{last}}}"),
            ),
            Some((last, rest)) => {
                let (listed, all) = (rest.join(", "), values.join(", "));
                self.either(
                    format_args!("several states fit these inputs: {symbol} = {listed} or {last}"),
                    format_args!("{symbol} ∈ {{{all}}}"),
                )
            }
        }
    }

    fn undefined(&mut self, prop: Prop, phase: Phase) -> fmt::Result {
        let (words, key, phase) = (PropWords(prop), PropName(prop), phase_name(phase));
        self.either(format_args!("{words} is undefined for a {phase} state"), format_args!("{key}: undefined, {phase}"))
    }
}

/// A limit's owner in the long form: `Water's`, or `the` where the caller named none.
struct Whose<'a>(Option<&'a str>);

impl fmt::Display for Whose<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(subject) => write!(f, "{subject}'s"),
            None => f.write_str("the"),
        }
    }
}

/// The stability condition a state fails, as a message writes it.
const fn instability(condition: Instability) -> &'static str {
    match condition {
        Instability::DpDrhoAtT => "(∂p/∂ρ)_T ≤ 0",
        Instability::Cv => "c_v ≤ 0",
        Instability::DpDrhoAtS => "(∂p/∂ρ)_s ≤ 0",
        Instability::GibbsVolume => "g_p ≤ 0",
        Instability::GibbsCompressibility => "g_pp ≥ 0",
    }
}

/// A phase as a message names it, before "state".
const fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Liquid => "liquid",
        Phase::Gas => "gas",
        Phase::TwoPhase => "two-phase",
        Phase::Supercritical => "supercritical",
        Phase::SupercriticalGas => "supercritical gas",
        Phase::SupercriticalLiquid => "supercritical liquid",
        Phase::CriticalPoint => "critical-point",
        Phase::Solid => "solid",
    }
}

/// A property in the long form: its words and its key (`the isobaric heat capacity (Cpmass)`), a partial derivative
/// after "the derivative" (user decision EM7).
struct PropWords(Prop);

impl fmt::Display for PropWords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = PropName(self.0);
        match words::prop(self.0) {
            Some(words) => write!(f, "the {words} ({key})"),
            None => write!(f, "the derivative {key}"),
        }
    }
}

/// A property by its variant's name (`Cpmass`), a partial derivative as `∂Hmass/∂T|P`.
struct PropName(Prop);

impl fmt::Display for PropName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Prop::Partial(partial) => write!(f, "{}", PartialName(partial)),
            Prop::SecondPartial(second) => {
                write!(f, "∂({})/∂{:?}|{:?}", PartialName(second.first), second.wrt, second.at)
            }
            Prop::Constant(constant) => write!(f, "{constant:?}"),
            other => write!(f, "{other:?}"),
        }
    }
}

struct PartialName(Partial);

impl fmt::Display for PartialName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Partial { of, wrt, at } = self.0;
        write!(f, "∂{of:?}/∂{wrt:?}|{at:?}")
    }
}
