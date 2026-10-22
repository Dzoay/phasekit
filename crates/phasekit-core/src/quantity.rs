//! What a number in an [`Error`](crate::Error) is, and the units a caller writes it in (issue #191; user decisions
//! EM1-EM10). Core's API is SI (D5) and the kernel keeps no global state, so core cannot know a viewer's units: a caller
//! passes them, keyed by [`Dimension`], and [`Si`] writes SI.

use core::fmt;

use crate::units::Basis;

/// The kind of unit a quantity is written in: what a caller's [`Units`] are keyed by. A specific quantity carries its
/// basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Dimension {
    /// K.
    Temperature,
    /// Pa.
    Pressure,
    /// mol/m³ or kg/m³.
    Density(Basis),
    /// J/mol or J/kg: enthalpy, internal energy.
    Energy(Basis),
    /// J/(mol·K) or J/(kg·K): entropy, heat capacity.
    Entropy(Basis),
    /// m³/mol or m³/kg.
    SpecificVolume(Basis),
    /// kg/mol.
    MolarMass,
    /// A fraction in [0, 1]: a quality, a mole fraction, a relative humidity.
    Fraction,
    /// kg/kg: a humidity ratio.
    MassRatio,
    /// A pure number without bounds.
    Dimensionless,
}

impl Dimension {
    /// The SI unit's label.
    pub const fn si_label(self) -> &'static str {
        match self {
            Dimension::Temperature => "K",
            Dimension::Pressure => "Pa",
            Dimension::Density(basis) => by_basis(basis, "mol/m³", "kg/m³"),
            Dimension::Energy(basis) => by_basis(basis, "J/mol", "J/kg"),
            Dimension::Entropy(basis) => by_basis(basis, "J/(mol·K)", "J/(kg·K)"),
            Dimension::SpecificVolume(basis) => by_basis(basis, "m³/mol", "m³/kg"),
            Dimension::MolarMass => "kg/mol",
            Dimension::MassRatio => "kg/kg",
            Dimension::Fraction | Dimension::Dimensionless => "",
        }
    }

    /// Whether a value of this dimension must be positive (an absolute temperature, a pressure, a density).
    pub(crate) const fn is_positive(self) -> bool {
        matches!(
            self,
            Dimension::Temperature
                | Dimension::Pressure
                | Dimension::Density(_)
                | Dimension::SpecificVolume(_)
                | Dimension::MolarMass
        )
    }
}

/// A specific quantity's molar and mass forms.
const fn specific(name: &'static str, symbol: &'static str, [molar, mass]: [Dimension; 2]) -> [Quantity; 2] {
    [Quantity::new(name, symbol, molar), Quantity::new(name, symbol, mass)]
}

const fn by_basis_ref(both: &'static [Quantity; 2], basis: Basis) -> &'static Quantity {
    let [molar, mass] = both;
    match basis {
        Basis::Molar => molar,
        Basis::Mass => mass,
    }
}

const fn by_basis(basis: Basis, molar: &'static str, mass: &'static str) -> &'static str {
    match basis {
        Basis::Molar => molar,
        Basis::Mass => mass,
    }
}

/// A named physical quantity: what an error's number is. An error holds a `&'static Quantity`, so it stays small: core's
/// are constants and constructors here, and a family crate defines its own as a constant,
/// `const W: &Quantity = &Quantity::new("humidity ratio", "W", Dimension::MassRatio);`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Quantity {
    name: &'static str,
    symbol: &'static str,
    dimension: Dimension,
}

impl Quantity {
    /// Temperature, T, K.
    pub const TEMPERATURE: &'static Quantity = &Quantity::new("temperature", "T", Dimension::Temperature);
    /// Pressure, p, Pa.
    pub const PRESSURE: &'static Quantity = &Quantity::new("pressure", "p", Dimension::Pressure);
    /// Vapour quality, Q.
    pub const QUALITY: &'static Quantity = &Quantity::new("quality", "Q", Dimension::Fraction);
    /// A component's mole fraction, x.
    pub const MOLE_FRACTION: &'static Quantity = &Quantity::new("mole fraction", "x", Dimension::Fraction);
    /// Molar mass, M, kg/mol.
    pub const MOLAR_MASS: &'static Quantity = &Quantity::new("molar mass", "M", Dimension::MolarMass);

    /// A quantity named `name` (lower case, as a sentence writes it), written `symbol` in the short form.
    pub const fn new(name: &'static str, symbol: &'static str, dimension: Dimension) -> Self {
        Self { name, symbol, dimension }
    }

    /// Density, ρ, in `basis`.
    pub const fn density(basis: Basis) -> &'static Self {
        const BY_BASIS: [Quantity; 2] =
            specific("density", "ρ", [Dimension::Density(Basis::Molar), Dimension::Density(Basis::Mass)]);
        by_basis_ref(&BY_BASIS, basis)
    }

    /// Enthalpy, h, in `basis`.
    pub const fn enthalpy(basis: Basis) -> &'static Self {
        const BY_BASIS: [Quantity; 2] =
            specific("enthalpy", "h", [Dimension::Energy(Basis::Molar), Dimension::Energy(Basis::Mass)]);
        by_basis_ref(&BY_BASIS, basis)
    }

    /// Entropy, s, in `basis`.
    pub const fn entropy(basis: Basis) -> &'static Self {
        const BY_BASIS: [Quantity; 2] =
            specific("entropy", "s", [Dimension::Entropy(Basis::Molar), Dimension::Entropy(Basis::Mass)]);
        by_basis_ref(&BY_BASIS, basis)
    }

    /// Internal energy, u, in `basis`.
    pub const fn internal_energy(basis: Basis) -> &'static Self {
        const BY_BASIS: [Quantity; 2] =
            specific("internal energy", "u", [Dimension::Energy(Basis::Molar), Dimension::Energy(Basis::Mass)]);
        by_basis_ref(&BY_BASIS, basis)
    }

    /// The name a long message writes.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The symbol a short message writes.
    pub const fn symbol(self) -> &'static str {
        self.symbol
    }

    /// The kind of unit it is written in.
    pub const fn dimension(self) -> Dimension {
        self.dimension
    }
}

/// A unit: SI value = value · `scale` + `offset` (°C: scale 1, offset 273.15).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit<'a> {
    /// What follows the number ("°C", "bar"); empty for a pure number.
    pub label: &'a str,
    /// SI per unit.
    pub scale: f64,
    /// The SI value of the unit's zero.
    pub offset: f64,
}

impl<'a> Unit<'a> {
    /// The SI unit labelled `label`.
    pub const fn si(label: &'a str) -> Self {
        Self { label, scale: 1.0, offset: 0.0 }
    }

    /// An SI value in this unit.
    pub fn from_si(self, value: f64) -> f64 {
        (value - self.offset) / self.scale
    }
}

/// The units a caller reads: one per dimension. A message writes each of its numbers in the unit of its quantity's
/// dimension.
pub trait Units {
    /// The unit a value of `dimension` is written in.
    fn unit(&self, dimension: Dimension) -> Unit<'_>;
}

/// SI, every dimension in its SI unit: what `Display` writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Si;

impl Units for Si {
    fn unit(&self, dimension: Dimension) -> Unit<'_> {
        Unit::si(dimension.si_label())
    }
}

/// How a message reads (user decision EM2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Style {
    /// In words, for a sentence's room: `temperature 200 K is below the minimum, 273.16 K`.
    #[default]
    Long,
    /// The relation the value breaks, for a narrow place: `T = 200 K < T_min = 273.16 K`.
    Short,
}

/// An SI value written in `unit`: six significant figures, then the label.
pub(crate) struct InUnit<'a>(pub f64, pub Unit<'a>);

impl fmt::Display for InUnit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let InUnit(value, unit) = *self;
        let number = Sig(unit.from_si(value));
        if unit.label.is_empty() { write!(f, "{number}") } else { write!(f, "{number} {}", unit.label) }
    }
}

/// A number to six significant figures, trailing zeros dropped (user decision EM4): a decimal from 1e-4 to below 1e6,
/// otherwise `2e9`; NaN and ∞ by name, and a minus sign, not a hyphen.
pub(crate) struct Sig(pub f64);

impl fmt::Display for Sig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = self.0;
        let sign = if x.is_sign_negative() && x != 0.0 { "−" } else { "" };
        if x.is_nan() {
            return f.write_str("NaN");
        }
        if x.is_infinite() {
            return write!(f, "{sign}∞");
        }
        let (digits, exponent) = significand(x.abs());
        f.write_str(sign)?;
        if (-4..6).contains(&exponent) { decimal(f, &digits, exponent) } else { scientific(f, &digits, exponent) }
    }
}

/// The six significant digits of `x` ≥ 0, trailing zeros dropped ("0" for zero), and its decimal exponent.
fn significand(x: f64) -> (String, i32) {
    let sci = format!("{x:.5e}");
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let trimmed = digits.trim_end_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    (digits.to_string(), exponent.parse().unwrap_or(0))
}

/// `digits` × 10^(exponent − its length + 1) as a decimal: `647.096`, `0.0012`, `100000`.
fn decimal(f: &mut fmt::Formatter<'_>, digits: &str, exponent: i32) -> fmt::Result {
    let Some(whole) = usize::try_from(exponent.saturating_add(1)).ok().filter(|&w| w > 0) else {
        let zeros = usize::try_from(exponent.saturating_neg().saturating_sub(1)).unwrap_or(0);
        return write!(f, "0.{}{digits}", "0".repeat(zeros));
    };
    let (int, frac) = digits.split_at(whole.min(digits.len()));
    let pad = "0".repeat(whole.saturating_sub(digits.len()));
    if frac.is_empty() { write!(f, "{int}{pad}") } else { write!(f, "{int}{pad}.{frac}") }
}

/// `digits` with the point after the first digit, then the exponent: `2e9`, `1.5e−5`.
fn scientific(f: &mut fmt::Formatter<'_>, digits: &str, exponent: i32) -> fmt::Result {
    let (first, rest) = digits.split_at(1.min(digits.len()));
    let point = if rest.is_empty() { "" } else { "." };
    // `scientific` writes only numbers outside [1e-4, 1e6), so its exponent is never 0.
    // equivalent mutant: replace < with <=
    let sign = if exponent < 0 { "−" } else { "" };
    write!(f, "{first}{point}{rest}e{sign}{}", exponent.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(x: f64) -> String {
        Sig(x).to_string()
    }

    /// User decision EM4: six significant figures, trailing zeros dropped, a decimal from 1e-4 to below 1e6.
    #[test]
    fn six_significant_figures() {
        assert_eq!(sig(647.0959999999873), "647.096");
        assert_eq!(sig(273.16), "273.16");
        assert_eq!(sig(0.0), "0");
        assert_eq!(sig(-5.0), "−5");
        assert_eq!(sig(1.5), "1.5");
        assert_eq!(sig(0.5), "0.5");
        assert_eq!(sig(0.00012345678), "0.000123457");
        assert_eq!(sig(999999.4), "999999");
        assert_eq!(sig(100000.0), "100000");
        assert_eq!(sig(-273.15), "−273.15");
    }

    /// Outside [1e-4, 1e6) the number is written with an exponent, after rounding: 999999.7 rounds up to 1e6.
    #[test]
    fn large_and_small_numbers_take_an_exponent() {
        assert_eq!(sig(2e9), "2e9");
        assert_eq!(sig(1.0025e9), "1.0025e9");
        assert_eq!(sig(999999.7), "1e6");
        assert_eq!(sig(1.5e-5), "1.5e−5");
        assert_eq!(sig(-6.02214076e23), "−6.02214e23");
    }

    #[test]
    fn nan_and_infinity_by_name() {
        assert_eq!(sig(f64::NAN), "NaN");
        assert_eq!(sig(f64::INFINITY), "∞");
        assert_eq!(sig(f64::NEG_INFINITY), "−∞");
        assert_eq!(sig(-0.0), "0");
    }

    /// A unit's offset and scale: −273.15 °C is 0 K, and 10⁵ Pa is 1 bar.
    #[test]
    fn a_value_is_written_in_its_unit() {
        let celsius = Unit { label: "°C", scale: 1.0, offset: 273.15 };
        assert_eq!(InUnit(0.0, celsius).to_string(), "−273.15 °C");
        assert_eq!(InUnit(1e5, Unit { label: "bar", scale: 1e5, offset: 0.0 }).to_string(), "1 bar");
        assert_eq!(InUnit(0.5, Si.unit(Dimension::Fraction)).to_string(), "0.5");
        assert_eq!(InUnit(300.0, Si.unit(Dimension::Temperature)).to_string(), "300 K");
    }

    #[test]
    fn si_labels_carry_their_basis() {
        assert_eq!(Dimension::Energy(Basis::Mass).si_label(), "J/kg");
        assert_eq!(Dimension::Entropy(Basis::Molar).si_label(), "J/(mol·K)");
        assert_eq!(Quantity::density(Basis::Mass).dimension().si_label(), "kg/m³");
    }
}
