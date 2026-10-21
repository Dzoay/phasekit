//! An [`Error`]'s message in a caller's units and style (issue #191; user decisions EM1-EM6). Each number is written
//! in the unit of its quantity's dimension, to six significant figures; the error's fields keep the exact value.

use core::fmt;

use super::{DomainError, Error, Roots};
use crate::prop::{Partial, Prop};
use crate::quantity::{Dimension, InUnit, Quantity, Style, Units};
use crate::state::Phase;

/// An error written in a caller's units and style ([`Error::display`]).
pub struct Message<'a> {
    error: &'a Error,
    units: &'a dyn Units,
    style: Style,
}

impl<'a> Message<'a> {
    pub(super) fn new(error: &'a Error, units: &'a dyn Units, style: Style) -> Self {
        Self { error, units, style }
    }
}

impl fmt::Display for Message<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = Out { f, units: self.units, short: self.style == Style::Short };
        match self.error {
            Error::InvalidInput { quantity, value } => out.invalid_input(quantity, *value),
            Error::InvalidArgument { argument, value } => out.invalid_argument(argument, *value),
            // The site reads a mixture's liquid-liquid refusal by this long form (`site/src/lib/mixtures.ts`, user
            // decision MX4): it stays as it was.
            Error::Unsupported { pair } => out.either(
                format_args!("input pair {pair:?} is not supported by this model"),
                format_args!("{pair:?}: unsupported"),
            ),
            Error::Domain(e) => out.domain(e),
            Error::NoConvergence { strategy, iterations } => out.either(
                format_args!("{strategy:?} did not converge in {iterations} iterations"),
                format_args!("{strategy:?}: no convergence in {iterations} iterations"),
            ),
            Error::Ambiguous { roots } => out.ambiguous(roots),
            Error::Undefined { prop, phase } => out.undefined(*prop, *phase),
            Error::NoModel { prop } => {
                let prop = PropName(*prop);
                out.either(format_args!("no model for {prop}"), format_args!("{prop}: no model"))
            }
            Error::ForeignState => {
                out.either(format_args!("the state was produced by a different model"), format_args!("foreign state"))
            }
            Error::InvalidState { reason } => out.f.write_fmt(format_args!("inconsistent state: {reason}")),
            Error::Load(e) => out.either(format_args!("load error: {e}"), format_args!("{e}")),
            Error::Shape { expected, found } => out.either(
                format_args!("buffer length {found}, expected {expected}"),
                format_args!("length {found} ≠ {expected}"),
            ),
        }
    }
}

/// Where a message is written, in which units and style.
struct Out<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    units: &'a dyn Units,
    short: bool,
}

/// Which rule an invalid value breaks; the first that it does, so a message never claims a rule the value keeps.
enum Breach {
    NotANumber,
    Infinite,
    AbsoluteZero,
    NotPositive,
    OutsideUnitInterval,
    OutOfRange,
}

impl Breach {
    fn of(quantity: &Quantity, value: f64) -> Self {
        let dimension = quantity.dimension();
        if value.is_nan() {
            Breach::NotANumber
        } else if value.is_infinite() {
            Breach::Infinite
        } else if dimension == Dimension::Temperature && value <= 0.0 {
            Breach::AbsoluteZero
        } else if dimension.is_positive() && value <= 0.0 {
            Breach::NotPositive
        } else if dimension == Dimension::Fraction && !(0.0..=1.0).contains(&value) {
            Breach::OutsideUnitInterval
        } else {
            Breach::OutOfRange
        }
    }
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
        let breach = Breach::of(quantity, value);
        let (v, zero, one) = (self.value(quantity, value), self.value(quantity, 0.0), self.value(quantity, 1.0));
        if self.short {
            let symbol = quantity.symbol();
            match breach {
                Breach::NotANumber => write!(self.f, "{symbol} = NaN"),
                Breach::Infinite => write!(self.f, "{symbol} = {v}"),
                Breach::AbsoluteZero | Breach::NotPositive => write!(self.f, "{symbol} = {v} ≤ {zero}"),
                Breach::OutsideUnitInterval => write!(self.f, "{symbol} = {v} ∉ [{zero}, {one}]"),
                Breach::OutOfRange => write!(self.f, "{symbol} = {v}: out of range"),
            }
        } else {
            let name = quantity.name();
            match breach {
                Breach::NotANumber => write!(self.f, "{name} is not a number"),
                Breach::Infinite => write!(self.f, "{name} {v} is not finite"),
                Breach::AbsoluteZero => write!(self.f, "{name} {v} is not above absolute zero"),
                Breach::NotPositive => write!(self.f, "{name} {v} is not positive"),
                Breach::OutsideUnitInterval => write!(self.f, "{name} {v} is outside [{zero}, {one}]"),
                Breach::OutOfRange => write!(self.f, "{name} {v} is out of range"),
            }
        }
    }

    fn invalid_argument(&mut self, argument: &str, value: Option<usize>) -> fmt::Result {
        match value {
            Some(n) => write!(self.f, "invalid {argument}: {n}"),
            None => write!(self.f, "invalid {argument}"),
        }
    }

    fn domain(&mut self, e: &DomainError) -> fmt::Result {
        let (t, p) = (Quantity::TEMPERATURE, Quantity::PRESSURE);
        let bound = match *e {
            DomainError::BelowMinTemperature { t: value, t_min } => {
                Bound { quantity: t, value, limit: t_min, above: false, name: "minimum", subscript: "min" }
            }
            DomainError::AboveMaxTemperature { t: value, t_max } => {
                Bound { quantity: t, value, limit: t_max, above: true, name: "maximum", subscript: "max" }
            }
            DomainError::BelowMinPressure { p: value, p_min } => {
                Bound { quantity: p, value, limit: p_min, above: false, name: "minimum", subscript: "min" }
            }
            DomainError::AboveMaxPressure { p: value, p_max } => {
                Bound { quantity: p, value, limit: p_max, above: true, name: "maximum", subscript: "max" }
            }
            DomainError::BelowMeltingLine { t: value, t_melt } => Bound {
                quantity: t,
                value,
                limit: t_melt,
                above: false,
                name: "melting temperature at this pressure",
                subscript: "melt",
            },
            DomainError::MechanicallyUnstable => {
                return self.either(
                    format_args!("the state is mechanically unstable: dp/dρ ≤ 0 or c_v ≤ 0"),
                    format_args!("unstable: dp/dρ ≤ 0 or c_v ≤ 0"),
                );
            }
        };
        self.bound(&bound)
    }

    fn bound(&mut self, b: &Bound) -> fmt::Result {
        let (name, symbol) = (b.quantity.name(), b.quantity.symbol());
        let (v, limit) = (self.value(b.quantity, b.value), self.value(b.quantity, b.limit));
        let (side, relation) = if b.above { ("above", ">") } else { ("below", "<") };
        let (what, subscript) = (b.name, b.subscript);
        self.either(
            format_args!("{name} {v} is {side} the {what}, {limit}"),
            format_args!("{symbol} = {v} {relation} {symbol}_{subscript} = {limit}"),
        )
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
        let (prop, phase) = (PropName(prop), phase_name(phase));
        self.either(format_args!("{prop} is undefined for a {phase} state"), format_args!("{prop}: undefined, {phase}"))
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
