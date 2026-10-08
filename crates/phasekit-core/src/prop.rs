//! Output keys for the dynamic paths (batch, strings, C, JS). Typed getters on `State` are the main API;
//! `Prop` exists so one exhaustive table ([`crate::Fluid::prop`]) serves every dynamic caller (map 01 R5).
//! Its discriminants are not an ABI: `phasekit-capi` maps names, never integers (map 01 R1).

/// A property that a dynamic caller can request. Grows additively per milestone (M5: residual parts and
/// virials; M7: second partial derivatives and the fundamental derivative; M10: the full CoolProp table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // CoolProp's output names; units are SI (molar or mass as named)
pub enum Prop {
    T,
    P,
    Q,
    Dmolar,
    Dmass,
    Hmolar,
    Hmass,
    Smolar,
    Smass,
    Umolar,
    Umass,
    Gmolar,
    Gmass,
    Helmholtzmolar,
    Helmholtzmass,
    Cvmolar,
    Cvmass,
    Cpmolar,
    Cpmass,
    SpeedOfSound,
    /// Compressibility factor p/(ρRT).
    Z,
    /// Ideal-gas isobaric heat capacity: needs the model's ideal part (`ThermoModel::derivs`).
    Cp0molar,
    Cp0mass,
    /// Residual enthalpy RT·(A10^r + A01^r): needs the model's residual part (`ThermoModel::derivs`).
    HmolarResidual,
    /// Residual entropy R·(A10^r − A00^r).
    SmolarResidual,
    /// Residual Gibbs energy RT·(A00^r + A01^r).
    GmolarResidual,
    /// Second virial coefficient B, m³/mol: the exact Taylor coefficient of α^r at δ = 0
    /// (`HelmholtzModel::zero_density`), never a small-δ estimate (ROT-063).
    Bvirial,
    /// Third virial coefficient C, m⁶/mol².
    Cvirial,
    /// dB/dT, m³/(mol K).
    DBvirialDT,
    /// dC/dT, m⁶/(mol² K).
    DCvirialDT,
    MolarMass,
    Viscosity,
    Conductivity,
    SurfaceTension,
    /// A first partial derivative `(∂of/∂wrt)_at` (CoolProp `d(X)/d(Y)|Z`), from the stored order-2 bundle,
    /// so it works for every family, Gibbs solids included (E1).
    Partial(Partial),
}

/// A first-order state function a partial derivative can be taken of, with respect to, or at constant
/// (CoolProp's 12 first-order derivative variables, map 01 §4a). Cv, Cp, w, τ and δ need order-3 bundles
/// and join at M7 (additive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // CoolProp's names
pub enum DerivVar {
    T,
    P,
    Dmolar,
    Dmass,
    Hmolar,
    Hmass,
    Smolar,
    Smass,
    Umolar,
    Umass,
    Gmolar,
    Gmass,
}

/// `(∂of/∂wrt)_at` at a single-phase state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Partial {
    /// The differentiated variable.
    pub of: DerivVar,
    /// The variable it is differentiated by.
    pub wrt: DerivVar,
    /// The variable held constant.
    pub at: DerivVar,
}
