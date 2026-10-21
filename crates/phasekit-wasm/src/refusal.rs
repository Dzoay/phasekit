//! A refusal as data (PLAN.md M28.2; issue #191; user decisions EM1, EM2): a refused call throws a JS `Error` whose
//! message is core's long form in SI and which carries the refusal's code, its quantity and its numbers, and a
//! `refusal` whose `explain(units, style)` writes core's wording in the caller's units.

use phasekit_compat::CompatError;
use phasekit_core::{Basis, Dimension, DomainError, Error, Quantity, Style, Unit, Units};
use wasm_bindgen::prelude::*;

// The three JS functions a refusal needs, declared here rather than through js-sys, whose declarations of every JS
// built-in would add 0.4 MB to the module `gates features` measures (wasm-bindgen strips them from the shipped one).
#[wasm_bindgen]
extern "C" {
    /// `new Error(message)`.
    #[wasm_bindgen(js_name = Error)]
    type JsErrorObject;
    #[wasm_bindgen(constructor, js_class = "Error")]
    fn new(message: &str) -> JsErrorObject;
    /// `Reflect.get(target, key)`; `undefined` where `target` is not an object.
    #[wasm_bindgen(js_namespace = Reflect, js_name = get, catch)]
    fn reflect_get(target: &JsValue, key: &str) -> Result<JsValue, JsValue>;
    /// `Reflect.set(target, key, value)`.
    #[wasm_bindgen(js_namespace = Reflect, js_name = set)]
    fn reflect_set(target: &JsValue, key: &str, value: &JsValue) -> bool;
    /// `Reflect.set(target, key, numbers)`, the numbers a `Float64Array`.
    #[wasm_bindgen(js_namespace = Reflect, js_name = set)]
    fn reflect_set_numbers(target: &JsValue, key: &str, numbers: Vec<f64>) -> bool;
}

/// `Reflect.get(target, key)`, `undefined` where it throws.
fn get(target: &JsValue, key: &str) -> JsValue {
    reflect_get(target, key).unwrap_or(JsValue::UNDEFINED)
}

/// Every dimension a refusal's number can have, by the key a caller's units name it with: the site's kinds
/// (`site/src/lib/units.ts`) where it has one.
const DIMENSIONS: [(Dimension, &str); 14] = [
    (Dimension::Temperature, "temperature"),
    (Dimension::Pressure, "pressure"),
    (Dimension::Density(Basis::Molar), "molarDensity"),
    (Dimension::Density(Basis::Mass), "massDensity"),
    (Dimension::Energy(Basis::Molar), "molarEnergy"),
    (Dimension::Energy(Basis::Mass), "specificEnergy"),
    (Dimension::Entropy(Basis::Molar), "molarEntropy"),
    (Dimension::Entropy(Basis::Mass), "specificEntropy"),
    (Dimension::SpecificVolume(Basis::Molar), "molarVolume"),
    (Dimension::SpecificVolume(Basis::Mass), "specificVolume"),
    (Dimension::MolarMass, "molarMass"),
    (Dimension::Fraction, "fraction"),
    (Dimension::MassRatio, "massRatio"),
    (Dimension::Dimensionless, "dimensionless"),
];

/// The key of `dimension`, or `None` for one this package does not know yet.
fn dimension_key(dimension: Dimension) -> Option<&'static str> {
    DIMENSIONS.iter().find(|(d, _)| *d == dimension).map(|(_, key)| *key)
}

/// Why a call was refused, as data: the `refusal` of the `Error` a refused call throws.
#[wasm_bindgen]
pub struct Refusal {
    error: CompatError,
}

#[wasm_bindgen]
impl Refusal {
    /// What kind of refusal: `invalid_input`, `below_min_temperature`, `ambiguous`, `unknown_output`, …
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> String {
        code(&self.error).to_string()
    }

    /// The name of the quantity its numbers are values of ("temperature"), if it has one.
    #[wasm_bindgen(getter)]
    pub fn quantity(&self) -> Option<String> {
        quantity_of(&self.error).map(|q| q.name().to_string())
    }

    /// That quantity's symbol ("T").
    #[wasm_bindgen(getter)]
    pub fn symbol(&self) -> Option<String> {
        quantity_of(&self.error).map(|q| q.symbol().to_string())
    }

    /// The key of that quantity's dimension ("temperature", "specificEnergy"), which `explain`'s units are keyed by.
    #[wasm_bindgen(getter)]
    pub fn dimension(&self) -> Option<String> {
        quantity_of(&self.error).and_then(|q| dimension_key(q.dimension())).map(String::from)
    }

    /// Its numbers, SI: the refused value; the value, then the limit it is beyond; or the roots of an ambiguous input.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        values(&self.error)
    }

    /// The message in `units` and `style` ("long" or "short"): `units` maps a dimension's key to `{label, scale,
    /// offset}`, SI = value · scale + offset; a dimension it lacks is written in SI.
    pub fn explain(&self, units: &JsValue, style: &str) -> Result<String, JsError> {
        let style = match style {
            "long" => Style::Long,
            "short" => Style::Short,
            other => return Err(JsError::new(&format!("`{other}` is not a style: \"long\" or \"short\""))),
        };
        let table = Table::read(units)?;
        Ok(match &self.error {
            CompatError::Core(e) => e.display(&table, style).to_string(),
            other => other.to_string(),
        })
    }
}

/// The `Error` a refused call throws: its message the long form in SI, its fields the refusal's.
pub(crate) fn refused(error: CompatError) -> JsValue {
    let thrown: JsValue = JsErrorObject::new(&error.to_string()).into();
    let refusal = Refusal { error };
    // Setting a property of a fresh `Error` cannot fail; were it to, the message alone is still thrown.
    let _ = reflect_set_numbers(&thrown, "values", refusal.values());
    let fields: [(&str, JsValue); 4] = [
        ("code", refusal.code().into()),
        ("quantity", refusal.quantity().into()),
        ("dimension", refusal.dimension().into()),
        ("refusal", refusal.into()),
    ];
    for (key, value) in fields {
        let _ = reflect_set(&thrown, key, &value);
    }
    thrown
}

fn code(error: &CompatError) -> &'static str {
    match error {
        CompatError::UnknownOutput(_) => "unknown_output",
        CompatError::UnknownInput(_) => "unknown_input",
        CompatError::InvalidPair(..) => "invalid_pair",
        CompatError::InvalidFluid(_) => "invalid_fluid",
        CompatError::NotAConstant(_) => "not_a_constant",
        CompatError::NotAProperty(_) => "not_a_property",
        CompatError::Core(e) => core_code(e),
        _ => "other",
    }
}

fn core_code(error: &Error) -> &'static str {
    match error {
        Error::InvalidInput { .. } => "invalid_input",
        Error::InvalidArgument { .. } => "invalid_argument",
        Error::Unsupported { .. } => "unsupported",
        Error::Domain(e) => domain_code(e),
        Error::NoConvergence { .. } => "no_convergence",
        Error::Ambiguous { .. } => "ambiguous",
        Error::Undefined { .. } => "undefined",
        Error::NoModel { .. } => "no_model",
        Error::ForeignState => "foreign_state",
        Error::InvalidState { .. } => "invalid_state",
        Error::Load(_) => "load",
        Error::Shape { .. } => "shape",
        _ => "other",
    }
}

fn domain_code(error: &DomainError) -> &'static str {
    match error {
        DomainError::BelowMinTemperature { .. } => "below_min_temperature",
        DomainError::AboveMaxTemperature { .. } => "above_max_temperature",
        DomainError::BelowMinPressure { .. } => "below_min_pressure",
        DomainError::AboveMaxPressure { .. } => "above_max_pressure",
        DomainError::BelowMeltingLine { .. } => "below_melting_line",
        DomainError::MechanicallyUnstable => "mechanically_unstable",
        _ => "domain",
    }
}

/// The quantity a refusal's numbers are values of.
fn quantity_of(error: &CompatError) -> Option<&'static Quantity> {
    let CompatError::Core(e) = error else { return None };
    match e {
        Error::InvalidInput { quantity, .. } => Some(*quantity),
        Error::Ambiguous { roots } => Some(roots.quantity()),
        Error::Domain(d) => domain_quantity(d),
        _ => None,
    }
}

fn domain_quantity(error: &DomainError) -> Option<&'static Quantity> {
    match error {
        DomainError::BelowMinTemperature { .. }
        | DomainError::AboveMaxTemperature { .. }
        | DomainError::BelowMeltingLine { .. } => Some(Quantity::TEMPERATURE),
        DomainError::BelowMinPressure { .. } | DomainError::AboveMaxPressure { .. } => Some(Quantity::PRESSURE),
        _ => None,
    }
}

fn values(error: &CompatError) -> Vec<f64> {
    let CompatError::Core(e) = error else { return Vec::new() };
    match e {
        Error::InvalidInput { value, .. } => vec![*value],
        Error::Ambiguous { roots } => roots.as_slice().to_vec(),
        Error::Domain(d) => domain_values(d),
        _ => Vec::new(),
    }
}

fn domain_values(error: &DomainError) -> Vec<f64> {
    match *error {
        DomainError::BelowMinTemperature { t, t_min } => vec![t, t_min],
        DomainError::AboveMaxTemperature { t, t_max } => vec![t, t_max],
        DomainError::BelowMinPressure { p, p_min } => vec![p, p_min],
        DomainError::AboveMaxPressure { p, p_max } => vec![p, p_max],
        DomainError::BelowMeltingLine { t, t_melt } => vec![t, t_melt],
        _ => Vec::new(),
    }
}

/// A caller's units, read once from its object.
struct Table {
    units: Vec<(Dimension, String, f64, f64)>,
}

impl Table {
    /// Reads `{key: {label, scale, offset}}` for each dimension it names; `undefined` or `null` is SI throughout.
    fn read(units: &JsValue) -> Result<Self, JsError> {
        let mut table = Table { units: Vec::new() };
        if units.is_undefined() || units.is_null() {
            return Ok(table);
        }
        for (dimension, key) in DIMENSIONS {
            let entry = get(units, key);
            if !entry.is_undefined() {
                table.units.push(unit_of(dimension, key, &entry)?);
            }
        }
        Ok(table)
    }
}

/// One `{label, scale, offset}` entry, refused unless its label is a string and its scale and offset finite numbers,
/// the scale not zero.
fn unit_of(dimension: Dimension, key: &str, entry: &JsValue) -> Result<(Dimension, String, f64, f64), JsError> {
    let field = |name: &str| get(entry, name);
    let (label, scale, offset) = (field("label").as_string(), field("scale").as_f64(), field("offset").as_f64());
    match (label, scale, offset) {
        (Some(label), Some(scale), Some(offset)) if scale.is_finite() && scale != 0.0 && offset.is_finite() => {
            Ok((dimension, label, scale, offset))
        }
        _ => Err(JsError::new(&format!("units.{key} is not {{label, scale, offset}} with a finite, non-zero scale"))),
    }
}

impl Units for Table {
    fn unit(&self, dimension: Dimension) -> Unit<'_> {
        self.units.iter().find(|(d, ..)| *d == dimension).map_or_else(
            || Unit::si(dimension.si_label()),
            |(_, label, scale, offset)| Unit { label, scale: *scale, offset: *offset },
        )
    }
}
