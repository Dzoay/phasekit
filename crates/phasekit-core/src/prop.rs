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
    /// Ideal-gas isobaric heat capacity at the state's T, in the dome too: needs the model's ideal part
    /// (`ThermoModel::derivs`, in the dome `ThermoModel::helmholtz`).
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
    /// The mass-basis vapour quality: the molar one for a pure or pseudo-pure fluid, whose phases share one molar mass.
    Qmass,
    /// τ = T_r/T at the published reducing state.
    Tau,
    /// δ = ρ/ρ_r at the published reducing state.
    Delta,
    /// The ideal-gas part of h at the state's T, J/mol, in the handle's gauge (h = h⁰ + h^r).
    HmolarIdealGas,
    /// The ideal-gas part of s at the state's (T, ρ), J/(mol K), in the handle's gauge.
    SmolarIdealGas,
    /// The ideal-gas part of u at the state's T, J/mol, in the handle's gauge.
    UmolarIdealGas,
    HmassIdealGas,
    SmassIdealGas,
    UmassIdealGas,
    Viscosity,
    Conductivity,
    SurfaceTension,
    /// The melting curve's pressure at the state's T, Pa (`ThermoModel::melting_pressure`).
    MeltingPressure,
    /// The melting curve's temperature at the state's p, K (`ThermoModel::melting_temperature`).
    MeltingTemperature,
    /// A first partial derivative `(∂of/∂wrt)_at` (CoolProp `d(X)/d(Y)|Z`), from the stored order-2 bundle,
    /// so it works for every family, Gibbs solids included (E1).
    Partial(Partial),
    /// A second partial derivative, a first partial differentiated once more (CoolProp `d(d(X)/d(Y)|Z)/d(W)|V`):
    /// order 3, from the model's `derivs` (E1).
    SecondPartial(SecondPartial),
    /// The fundamental derivative of gas dynamics Γ = 1 + (ρ/w)(∂w/∂ρ)_s, dimensionless: order 3.
    FundamentalDerivative,
    /// The isothermal compressibility κ_T = (1/ρ)(∂ρ/∂p)_T, 1/Pa.
    IsothermalCompressibility,
    /// The isobaric expansion coefficient β = −(1/ρ)(∂ρ/∂T)_p, 1/K.
    IsobaricExpansion,
    /// κ_s = (ρ/p)(∂p/∂ρ)_s, CoolProp's `isentropic_expansion_coefficient`.
    IsentropicExpansion,
    /// c_p·η/λ.
    Prandtl,
    /// The phase identification parameter of Venkatarathnam and Oellrich (2011), 2 − ρ[(∂²p/∂ρ∂T)/(∂p/∂T)_ρ −
    /// (∂²p/∂ρ²)_T/(∂p/∂ρ)_T]: above 1 a gas, below 1 a liquid.
    Pip,
    /// The τ- and δ-scaled α terms CoolProp outputs: α^r, ∂α^r/∂τ, ∂α^r/∂δ, α⁰, ∂α⁰/∂τ, ∂α⁰/∂δ, ∂²α⁰/∂δ², ∂³α⁰/∂δ³.
    Alphar,
    DalpharDtau,
    DalpharDdelta,
    Alpha0,
    Dalpha0Dtau,
    Dalpha0Ddelta,
    D2alpha0Ddelta2,
    D3alpha0Ddelta3,
    /// A state-independent output ([`crate::Fluid::constant`]).
    Constant(Constant),
}

/// A fluid's state-independent outputs: CoolProp's trivial parameters (map 01 §4a), from the model's limits, critical
/// point and published constants (`FluidInfo::constants`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // CoolProp's output names; units are SI
pub enum Constant {
    GasConstant,
    MolarMass,
    Acentric,
    DipoleMoment,
    RhomassReducing,
    RhomolarReducing,
    RhomolarCritical,
    RhomassCritical,
    TReducing,
    TCritical,
    /// The EOS's own triple point, not CoolProp's T_min (map 09 R8).
    TTriple,
    TMax,
    TMin,
    /// The saturation pressure at T_min (CoolProp's `P_min` and `p_triple`, map 09 R8).
    PMin,
    PMax,
    PCritical,
    PReducing,
    PTriple,
    /// An incompressible solution's fraction limits and freezing temperature.
    FractionMin,
    FractionMax,
    TFreeze,
    /// Environmental and safety metadata: restricted data, never shipped (D14), so `NoModel` from the shipped blobs.
    Gwp20,
    Gwp100,
    Gwp500,
    Fh,
    Hh,
    Ph,
    Odp,
}

/// A state function a partial derivative can be taken of, with respect to, or at constant: CoolProp's 12 first-order
/// derivative variables (map 01 §4a), c_v, c_p and w, and τ and δ (PLAN.md M20.1, user decision OD1).
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
    /// c_v, c_p and w, second-order state functions: their derivatives read the model's order-3 bundle
    /// (`ThermoModel::derivs`), so [`crate::Fluid::prop`] gives a partial of them and a single phase's stored bundle does
    /// not ([`crate::State::partial`]).
    Cvmolar,
    Cvmass,
    Cpmolar,
    Cpmass,
    SpeedOfSound,
    /// τ = T_r/T and δ = ρ/ρ_r at the published reducing state (`FluidInfo::constants`), which
    /// [`crate::Fluid::prop`] reads.
    Tau,
    Delta,
}

impl DerivVar {
    /// Whether this is c_v, c_p or w: a function of α's second derivatives, so its own derivatives are order 3 and a
    /// second partial's inner partial, which is differentiated once more, cannot take it from an order-3 bundle.
    pub const fn is_second_order(self) -> bool {
        matches!(self, Self::Cvmolar | Self::Cvmass | Self::Cpmolar | Self::Cpmass | Self::SpeedOfSound)
    }

    /// Whether this is τ or δ, taken at the published reducing state.
    pub(crate) const fn is_reduced(self) -> bool {
        matches!(self, Self::Tau | Self::Delta)
    }
}

/// A first-order variable is the property of the same name: what it means at a state is [`crate::Fluid::prop`]'s.
impl From<DerivVar> for Prop {
    fn from(v: DerivVar) -> Prop {
        match v {
            DerivVar::T => Prop::T,
            DerivVar::P => Prop::P,
            DerivVar::Dmolar => Prop::Dmolar,
            DerivVar::Dmass => Prop::Dmass,
            DerivVar::Hmolar => Prop::Hmolar,
            DerivVar::Hmass => Prop::Hmass,
            DerivVar::Smolar => Prop::Smolar,
            DerivVar::Smass => Prop::Smass,
            DerivVar::Umolar => Prop::Umolar,
            DerivVar::Umass => Prop::Umass,
            DerivVar::Gmolar => Prop::Gmolar,
            DerivVar::Gmass => Prop::Gmass,
            DerivVar::Cvmolar => Prop::Cvmolar,
            DerivVar::Cvmass => Prop::Cvmass,
            DerivVar::Cpmolar => Prop::Cpmolar,
            DerivVar::Cpmass => Prop::Cpmass,
            DerivVar::SpeedOfSound => Prop::SpeedOfSound,
            DerivVar::Tau => Prop::Tau,
            DerivVar::Delta => Prop::Delta,
        }
    }
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

/// `(∂/∂wrt (∂of/∂wrt₁)_at₁)_at` at a single-phase state: the first partial `first` differentiated by `wrt` at
/// constant `at`. `first` names no c_v, c_p or w ([`DerivVar::is_second_order`]): differentiated twice, they are order 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondPartial {
    /// The first partial differentiated.
    pub first: Partial,
    /// The variable it is differentiated by.
    pub wrt: DerivVar,
    /// The variable held constant.
    pub at: DerivVar,
}
