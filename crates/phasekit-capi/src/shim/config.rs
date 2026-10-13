//! `set_config_*` (user decision P6): CoolProp 8.0.0's 38 configuration keys, each with its type. Two have a phasekit
//! equivalent and take effect on every later shim call: `DONT_CHECK_PROPERTY_LIMITS`, the flash options'
//! `DomainPolicy::Extrapolate`, and `LIST_STRING_DELIMITER`, the delimiter of the shim's lists. Every other key is
//! accepted and ignored (docs/MIGRATION.md). Nothing reaches the kernel but per-call options.

use super::state;

/// A configuration value's type, as CoolProp 8.0.0 declares it (`configuration_keys.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `set_config_bool`.
    Bool,
    /// `set_config_double`.
    Double,
    /// An integer: no Tier A function sets one.
    Int,
    /// `set_config_string`.
    Text,
}

/// A value one of the three `set_config_*` functions passes.
#[derive(Clone, Copy, Debug)]
pub(super) enum Value<'a> {
    Bool(bool),
    Double,
    Text(&'a str),
}

impl Value<'_> {
    fn kind(self) -> Kind {
        match self {
            Value::Bool(_) => Kind::Bool,
            Value::Double => Kind::Double,
            Value::Text(_) => Kind::Text,
        }
    }
}

/// CoolProp 8.0.0's configuration keys and their types (the fixtures' `config:` header lists them).
const CONFIG: [(&str, Kind); 38] = [
    ("ALLOW_SVDSBTL_IN_PROPSSI", Kind::Bool),
    ("ALTERNATIVE_REFPROP_HMX_BNC_PATH", Kind::Text),
    ("ALTERNATIVE_REFPROP_LIBRARY_PATH", Kind::Text),
    ("ALTERNATIVE_REFPROP_PATH", Kind::Text),
    ("ALTERNATIVE_SVDTABLES_DIRECTORY", Kind::Text),
    ("ALTERNATIVE_TABLES_DIRECTORY", Kind::Text),
    ("ASSUME_CRITICAL_POINT_STABLE", Kind::Bool),
    ("CRITICAL_SPLINES_ENABLED", Kind::Bool),
    ("CRITICAL_WITHIN_1UK", Kind::Bool),
    ("DONT_CHECK_PROPERTY_LIMITS", Kind::Bool),
    ("ENABLE_MELTING_CALORIC_HS", Kind::Bool),
    ("ENABLE_SUPERANCILLARIES", Kind::Bool),
    ("FLOAT_PUNCTUATION", Kind::Text),
    ("HENRYS_LAW_TO_GENERATE_VLE_GUESSES", Kind::Bool),
    ("HSU_D_TWOPHASE_EOS_POLISH", Kind::Bool),
    ("LIST_STRING_DELIMITER", Kind::Text),
    ("MAXIMUM_TABLE_DIRECTORY_SIZE_IN_GB", Kind::Double),
    ("MIXTURE_STABILITY_ALGORITHM", Kind::Int),
    ("NORMALIZE_GAS_CONSTANTS", Kind::Bool),
    ("OVERWRITE_BINARY_INTERACTION", Kind::Bool),
    ("OVERWRITE_DEPARTURE_FUNCTION", Kind::Bool),
    ("OVERWRITE_FLUIDS", Kind::Bool),
    ("PHASE_ENVELOPE_STARTING_PRESSURE_PA", Kind::Double),
    ("REFPROP_DONT_ESTIMATE_INTERACTION_PARAMETERS", Kind::Bool),
    ("REFPROP_ERROR_THRESHOLD", Kind::Int),
    ("REFPROP_IGNORE_ERROR_ESTIMATED_INTERACTION_PARAMETERS", Kind::Bool),
    ("REFPROP_RESOLVE_COOLPROP_ALIASES", Kind::Bool),
    ("REFPROP_USE_GERG", Kind::Bool),
    ("REFPROP_USE_PENGROBINSON", Kind::Bool),
    ("R_U_CODATA", Kind::Double),
    ("SAVE_RAW_TABLES", Kind::Bool),
    ("SPINODAL_MINIMUM_DELTA", Kind::Double),
    ("SVDSBTL_SAMPLING_THREADS", Kind::Int),
    ("TABULAR_NX", Kind::Int),
    ("TABULAR_NY", Kind::Int),
    ("USE_GUESSES_IN_PROPSSI", Kind::Bool),
    ("VTPR_ALWAYS_RELOAD_LIBRARY", Kind::Bool),
    ("VTPR_UNIFAC_PATH", Kind::Text),
];

/// The configuration keys and their types.
pub fn config_keys() -> impl Iterator<Item = (&'static str, Kind)> {
    CONFIG.iter().copied()
}

/// Sets `key` to `value`: one of the two keys with an equivalent, or any other key, accepted and ignored. An unknown
/// key or a value of another type is refused, as CoolProp refuses it.
pub(super) fn set(key: &str, value: Value<'_>) -> Result<(), String> {
    let kind = CONFIG.iter().find(|(name, _)| *name == key).map(|(_, kind)| *kind);
    let kind = kind.ok_or_else(|| format!("`{key}` is not a CoolProp 8.0.0 configuration key"))?;
    if kind != value.kind() {
        return Err(format!("`{key}` takes a {kind:?} value, not a {:?} one", value.kind()));
    }
    match (key, value) {
        ("DONT_CHECK_PROPERTY_LIMITS", Value::Bool(on)) => state::update(|s| s.extrapolate = on),
        ("LIST_STRING_DELIMITER", Value::Text(text)) => state::update(|s| s.delimiter = text.to_string().into()),
        _ => {}
    }
    Ok(())
}
