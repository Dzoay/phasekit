//! The one serde mirror of CoolProp's fluid JSON (`dev/fluids/*.json`, v8.0.0; S-12: it lives here, not in core).
//!
//! Closed by construction (map 09 R7, map 10 R8): every struct denies unknown fields and every term list is an
//! internally tagged enum, so an unknown block `type`, a misspelt key or a missing required key is a parse error,
//! never a silently skipped term (CoolProp prints and ignores an unknown α⁰ type, `FluidLibrary.h:327-329`). The
//! mirror stays close to the file: documentation fields (`*_units`, `_note`) are carried so datagen can check them
//! (M2.3), and every EOS entry is mirrored, alternates included, so skipping them is explicit (ROT-041, ROT-068).
//! Subtrees no step reads yet stay `serde_json::Value` until the step that types them: ancillaries (M4.x, M6),
//! the superancillary expansions (M5.2), the critical-region splines (M6) and transport (M8).
#![expect(
    dead_code,
    reason = "a schema: every field exists so the closed structs accept the file; M2.2 reads the stamps and check \
              points, M2.3 the rest (and drops this)"
)]

use serde::Deserialize;
use serde_json::Value;

/// A number with its literal kind: exponents `d` and `l` are integer-valued, but 277 `d` and 161 `l` are written as
/// floats (`1.0`; map 02 §3.1), so the mirror takes both and M2.3 checks that each is integral.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub fn value(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(x) => x,
        }
    }
}

/// One file of `dev/fluids`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fluid {
    #[serde(rename = "INFO")]
    pub info: Info,
    /// `EOS[0]` is the default; the 23 alternates are never compiled (map 09 §4.1).
    #[serde(rename = "EOS")]
    pub eos: Vec<Eos>,
    #[serde(rename = "STATES")]
    pub states: States,
    #[serde(rename = "ANCILLARIES")]
    pub ancillaries: Value,
    #[serde(rename = "TRANSPORT")]
    pub transport: Option<Value>,
}

/// `INFO`: identity and metadata (map 09 §4.2).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    #[serde(rename = "NAME")]
    pub name: String,
    #[serde(rename = "CAS")]
    pub cas: String,
    #[serde(rename = "REFPROP_NAME")]
    pub refprop_name: String,
    #[serde(rename = "ALIASES")]
    pub aliases: Vec<String>,
    #[serde(rename = "FORMULA")]
    pub formula: Option<String>,
    #[serde(rename = "INCHI_KEY")]
    pub inchi_key: Option<String>,
    #[serde(rename = "INCHI_STRING")]
    pub inchi_string: Option<String>,
    #[serde(rename = "SMILES")]
    pub smiles: Option<String>,
    #[serde(rename = "CHEMSPIDER_ID")]
    pub chemspider_id: Option<i64>,
    #[serde(rename = "2DPNG_URL")]
    pub png_url: Option<String>,
    #[serde(rename = "ENVIRONMENTAL")]
    pub environmental: Option<Environmental>,
}

/// `INFO.ENVIRONMENTAL`, sentinels and all (−1, "UNKNOWN", "?"; M2.3 maps them to `None`, ROT-057).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environmental {
    #[serde(rename = "ASHRAE34")]
    pub ashrae34: String,
    #[serde(rename = "GWP20")]
    pub gwp20: f64,
    #[serde(rename = "GWP100")]
    pub gwp100: f64,
    #[serde(rename = "GWP500")]
    pub gwp500: f64,
    #[serde(rename = "ODP")]
    pub odp: f64,
    #[serde(rename = "HH")]
    pub hh: Num,
    #[serde(rename = "FH")]
    pub fh: Num,
    #[serde(rename = "PH")]
    pub ph: Num,
    #[serde(rename = "Name")]
    pub name: String,
}

/// Top-level `STATES`. `triple_liquid` is the EOS minimum state, not the triple point (map 09 §4.2).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct States {
    pub critical: StatePoint,
    pub triple_liquid: StatePoint,
    pub triple_vapor: StatePoint,
}

/// A stored state. `hmolar` and `smolar` are derived values (fossil in 57 fluids, map 10 R11), never shipped
/// (ROT-052); `reducing` carries them too.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatePoint {
    #[serde(rename = "T")]
    pub t: f64,
    #[serde(rename = "T_units")]
    pub t_units: String,
    pub p: f64,
    pub p_units: String,
    pub rhomolar: f64,
    pub rhomolar_units: String,
    pub hmolar: Option<f64>,
    pub hmolar_units: Option<String>,
    pub smolar: Option<f64>,
    pub smolar_units: Option<String>,
}

/// One `EOS` entry.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eos {
    #[serde(rename = "BibTeX_EOS")]
    pub bibtex_eos: String,
    #[serde(rename = "BibTeX_CP0")]
    pub bibtex_cp0: String,
    pub gas_constant: f64,
    pub gas_constant_units: String,
    pub molar_mass: f64,
    pub molar_mass_units: String,
    pub acentric: f64,
    pub acentric_units: String,
    pub acentric_note: Option<String>,
    #[serde(rename = "T_max")]
    pub t_max: f64,
    #[serde(rename = "T_max_units")]
    pub t_max_units: String,
    pub p_max: f64,
    pub p_max_units: String,
    /// Never read by CoolProp (map 09 R8); kept apart from `T_min` from M2.3 on (ROT-046).
    #[serde(rename = "Ttriple")]
    pub t_triple: f64,
    #[serde(rename = "Ttriple_units")]
    pub t_triple_units: String,
    pub pseudo_pure: bool,
    pub alphar: Vec<ResidualBlock>,
    pub alpha0: Vec<IdealBlock>,
    #[serde(rename = "STATES")]
    pub states: EosStates,
    #[serde(rename = "SUPERANCILLARY")]
    pub superancillary: Option<Superancillary>,
    pub critical_region_splines: Option<Value>,
}

/// `EOS[].STATES`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EosStates {
    pub reducing: StatePoint,
    pub sat_min_liquid: StatePoint,
    pub sat_min_vapor: StatePoint,
    /// Absent from three alternates (R11, R123, R152A).
    pub hs_anchor: Option<StatePoint>,
    pub pressure_max_sat: Option<StatePoint>,
    pub temperature_max_sat: Option<StatePoint>,
}

/// `EOS[].SUPERANCILLARY`. The default EOS of 130 fluids carries one with its freshness stamp and check points;
/// R1234yf's alternate carries one with neither (map 09 R10).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Superancillary {
    /// FNV-1a-64 of `EOS[0]` minus this subtree, as fastchebpure stamped it (map 09 §3; recomputed at M2.2).
    pub source_eos_hash: Option<String>,
    pub check_points: Option<Vec<CheckPoint>>,
    pub jexpansions_p: Value,
    #[serde(rename = "jexpansions_rhoL")]
    pub jexpansions_rho_l: Value,
    #[serde(rename = "jexpansions_rhoV")]
    pub jexpansions_rho_v: Value,
    pub crit_anc: Value,
    pub meta: Value,
}

/// One multiprecision saturation point (fastchebpure; map 09 §8), committed as `mp/check-points.csv` at M1.17.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPoint {
    #[serde(rename = "T / K")]
    pub t: f64,
    #[serde(rename = "p(mp) / Pa")]
    pub p: f64,
    #[serde(rename = "rho'(mp) / mol/m^3")]
    pub rho_l: f64,
    #[serde(rename = "rho''(mp) / mol/m^3")]
    pub rho_v: f64,
    #[serde(rename = "p(SA)/p(mp)")]
    pub p_ratio: f64,
    #[serde(rename = "rho'(SA)/rho'(mp)")]
    pub rho_l_ratio: f64,
    #[serde(rename = "rho''(SA)/rho''(mp)")]
    pub rho_v_ratio: f64,
}

/// `alphar` blocks (map 02 §3.1).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ResidualBlock {
    #[serde(rename = "ResidualHelmholtzPower")]
    Power { n: Vec<f64>, t: Vec<f64>, d: Vec<Num>, l: Vec<Num> },
    #[serde(rename = "ResidualHelmholtzExponential")]
    Exponential { n: Vec<f64>, t: Vec<f64>, d: Vec<Num>, l: Vec<Num>, g: Vec<f64> },
    #[serde(rename = "ResidualHelmholtzLemmon2005")]
    Lemmon2005 { n: Vec<f64>, t: Vec<f64>, d: Vec<Num>, l: Vec<Num>, m: Vec<f64> },
    #[serde(rename = "ResidualHelmholtzDoubleExponential")]
    DoubleExponential { n: Vec<f64>, t: Vec<f64>, d: Vec<Num>, gd: Vec<f64>, ld: Vec<Num>, gt: Vec<f64>, lt: Vec<f64> },
    #[serde(rename = "ResidualHelmholtzGaussian")]
    Gaussian {
        n: Vec<f64>,
        t: Vec<f64>,
        d: Vec<Num>,
        eta: Vec<f64>,
        epsilon: Vec<f64>,
        beta: Vec<f64>,
        gamma: Vec<f64>,
    },
    /// JSON η has the opposite sign to the paper (map 02 §3.1; normalised at M2.3).
    #[serde(rename = "ResidualHelmholtzGaoB")]
    GaoB {
        n: Vec<f64>,
        t: Vec<f64>,
        d: Vec<Num>,
        eta: Vec<f64>,
        epsilon: Vec<f64>,
        beta: Vec<f64>,
        gamma: Vec<f64>,
        b: Vec<f64>,
    },
    #[serde(rename = "ResidualHelmholtzNonAnalytic")]
    NonAnalytic {
        n: Vec<f64>,
        a: Vec<f64>,
        b: Vec<f64>,
        beta: Vec<f64>,
        #[serde(rename = "A")]
        big_a: Vec<f64>,
        #[serde(rename = "B")]
        big_b: Vec<f64>,
        #[serde(rename = "C")]
        big_c: Vec<f64>,
        #[serde(rename = "D")]
        big_d: Vec<f64>,
    },
    /// Only in Methanol's alternate `EOS[1]`; never compiled (ROT-068).
    #[serde(rename = "ResidualHelmholtzAssociating")]
    Associating { a: f64, m: f64, epsilonbar: f64, kappabar: f64, vbarn: f64 },
}

/// `alpha0` blocks (map 02 §3.2).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum IdealBlock {
    #[serde(rename = "IdealGasHelmholtzLead")]
    Lead {
        a1: f64,
        a2: f64,
        #[serde(rename = "_note")]
        note: Option<String>,
    },
    #[serde(rename = "IdealGasHelmholtzLogTau")]
    LogTau { a: f64 },
    #[serde(rename = "IdealGasHelmholtzPower")]
    Power { n: Vec<f64>, t: Vec<f64> },
    /// JSON `t` is θ with CoolProp's sign convention (map 02 §3.2; normalised at M2.3).
    #[serde(rename = "IdealGasHelmholtzPlanckEinstein")]
    PlanckEinstein { n: Vec<f64>, t: Vec<f64> },
    #[serde(rename = "IdealGasHelmholtzPlanckEinsteinGeneralized")]
    PlanckEinsteinGeneralized { n: Vec<f64>, t: Vec<f64>, c: Vec<f64>, d: Vec<f64> },
    /// θ = v / `Tcrit`; `R` and `T0` are present in two fluids and ignored by CoolProp (map 02 §3.2).
    #[serde(rename = "IdealGasHelmholtzPlanckEinsteinFunctionT")]
    PlanckEinsteinFunctionT {
        n: Vec<f64>,
        v: Vec<f64>,
        #[serde(rename = "Tcrit")]
        t_crit: f64,
        #[serde(rename = "Tcrit_units")]
        t_crit_units: Option<String>,
        #[serde(rename = "R")]
        r: Option<f64>,
        #[serde(rename = "T0")]
        t0: Option<f64>,
    },
    #[serde(rename = "IdealGasHelmholtzEnthalpyEntropyOffset")]
    EnthalpyEntropyOffset { a1: f64, a2: f64, reference: String },
    /// `R` is present in two fluids and ignored by CoolProp.
    #[serde(rename = "IdealGasHelmholtzCP0PolyT")]
    Cp0PolyT {
        c: Vec<f64>,
        t: Vec<f64>,
        #[serde(rename = "Tc")]
        tc: f64,
        #[serde(rename = "T0")]
        t0: f64,
        #[serde(rename = "R")]
        r: Option<f64>,
    },
    #[serde(rename = "IdealGasHelmholtzCP0Constant")]
    Cp0Constant {
        #[serde(rename = "cp_over_R")]
        cp_over_r: f64,
        #[serde(rename = "Tc")]
        tc: f64,
        #[serde(rename = "T0")]
        t0: f64,
    },
    /// Aly & Lee 1981; CoolProp folds its constant into the CP0PolyT container (`extend`, map 02 §3.2).
    #[serde(rename = "IdealGasHelmholtzCP0AlyLee")]
    Cp0AlyLee {
        c: Vec<f64>,
        #[serde(rename = "Tc")]
        tc: f64,
        #[serde(rename = "T0")]
        t0: f64,
    },
}

impl IdealBlock {
    /// `(Tc, T0)` of a c_p⁰ block CoolProp evaluates through its single CP0PolyT container.
    pub fn polyt_constants(&self) -> Option<(f64, f64)> {
        match *self {
            IdealBlock::Cp0PolyT { tc, t0, .. } | IdealBlock::Cp0AlyLee { tc, t0, .. } => Some((tc, t0)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A number keeps its literal kind and its value.
    #[test]
    fn numbers_keep_their_literal_kind() {
        let nums: Vec<Num> = serde_json::from_str("[3, 1.0, -2.5]").unwrap();
        assert_eq!(nums, vec![Num::Int(3), Num::Float(1.0), Num::Float(-2.5)]);
        assert_eq!(nums.iter().map(|n| n.value()).collect::<Vec<_>>(), vec![3.0, 1.0, -2.5]);
    }
}
