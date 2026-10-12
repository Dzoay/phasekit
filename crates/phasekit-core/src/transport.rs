//! L5 transport: CoolProp's correlation forms as closed sum types (map 05 U2), decoded from the blob's transport
//! section on a fluid's first transport call, so thermo-only users never read it or load its ECS reference fluids
//! (Lean fatal flaw; Kernel graft). Written only against the public `State` API (materials S5). The staged forms
//! evaluate from M8.5 (viscosity) and M8.6 (conductivity), the IAPWS releases from M8.7; a form a later step evaluates
//! says so, and a fluid without a model reports `NoModel`.
//!
//! The record is v8.0.0's data as datagen maps it (PLAN.md M8.1): each property's active entry only (a list's first,
//! ROT-109), with its own source; every form CoolProp can reach and none it cannot (ROT-116: no CO₂ Vesovic or
//! Scalabrin term, no `RESIDUAL_CO2`); fixed-size arrays where a form has a fixed size, and checked equal lengths where
//! it has parallel lists (map 05 R11). [`Part`] describes each type once, field by field, for the blob codec and
//! `cargo xtask fluid`'s dump alike.

mod conductivity;
mod ecs;
mod iapws;
mod viscosity;

pub use conductivity::ConductivityStages;
pub use iapws::{WaterConductivity, WaterViscosity};
pub(crate) use iapws::{water_conductivity, water_viscosity};
pub use viscosity::ViscosityStages;
pub(crate) use viscosity::{TransportInputs, lands};

use crate::data::{Ancillary, DataSet, Edit, Patch};
use crate::error::{DomainError, Error, LoadError};
use crate::fluid::Fluid;
use crate::math;
use crate::model::{DataTerms, Source};
use crate::prop::Prop;

/// A closed list of CoolProp's hardcoded models for one slot, by the name its JSON writes (`"hardcoded": "Water"`).
/// Tags are the names' positions in `NAMES`, never enum discriminants (ROT-055).
macro_rules! hardcoded {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// CoolProp's names of the models, in tag order.
            pub const NAMES: &'static [&'static str] = &[$($text),+];
            const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The model CoolProp's JSON names `name`, if it is one of this slot's.
            pub fn from_name(name: &str) -> Option<Self> {
                Self::NAMES.iter().position(|n| *n == name).and_then(|i| Self::ALL.get(i).copied())
            }

            /// CoolProp's name of the model.
            pub fn name(self) -> &'static str {
                Self::NAMES.get(self.index()).copied().unwrap_or_default()
            }

            fn index(self) -> usize {
                Self::ALL.iter().position(|x| *x == self).unwrap_or(usize::MAX)
            }
        }

        impl Part for $name {
            fn put(&self, p: &mut dyn Put) {
                p.choice("hardcoded", Self::NAMES, self.index());
            }

            fn take(t: &mut dyn Take) -> Result<Self, String> {
                let i = t.choice("hardcoded", Self::NAMES)?;
                Self::ALL.get(i).copied().ok_or_else(|| format!("hardcoded model {i} out of range"))
            }
        }
    };
}

hardcoded! {
    /// A whole viscosity model hard-coded in CoolProp (map 05 §3.8).
    HardcodedViscosity {
        /// IAPWS R12-08.
        Water = "Water",
        /// IAPWS heavy water (2007 release).
        HeavyWater = "HeavyWater",
        /// Arp, McCarty and Friend (1998).
        Helium = "Helium",
        /// Shan, Penoncello and Jacobsen (2000).
        R23 = "R23",
        /// Xiang, Laesecke and Huber (2006).
        Methanol = "Methanol",
        /// Cao et al. (2016).
        MXylene = "m-Xylene",
        /// Cao et al. (2016).
        OXylene = "o-Xylene",
        /// Balogun et al. (2015).
        PXylene = "p-Xylene",
    }
}

hardcoded! {
    /// A dilute-gas viscosity term hard-coded in CoolProp.
    HardcodedDiluteViscosity {
        /// Friend, Ingham and Ely (1991).
        Ethane = "Ethane",
        /// Tariq et al. (2014).
        Cyclohexane = "Cyclohexane",
        /// Laesecke and Muzny (2017).
        CarbonDioxideLaeseckeJpcrd2017 = "CarbonDioxideLaeseckeJPCRD2017",
    }
}

hardcoded! {
    /// A higher-order viscosity term hard-coded in CoolProp.
    HardcodedHigherViscosity {
        /// Muzny et al. (2013).
        Hydrogen = "Hydrogen",
        /// Michailidou et al. (2013).
        NHexane = "n-Hexane",
        /// Michailidou et al. (2014).
        NHeptane = "n-Heptane",
        /// Avgeri et al. (2015).
        Toluene = "Toluene",
        /// Friend, Ingham and Ely (1991).
        Ethane = "Ethane",
        /// Avgeri et al. (2014).
        Benzene = "Benzene",
        /// Laesecke and Muzny (2017).
        CarbonDioxideLaeseckeJpcrd2017 = "CarbonDioxideLaeseckeJPCRD2017",
    }
}

hardcoded! {
    /// A whole conductivity model hard-coded in CoolProp (map 05 §3.8).
    HardcodedConductivity {
        /// IAPWS R15-11.
        Water = "Water",
        /// IAPWS heavy water (2007 release).
        HeavyWater = "HeavyWater",
        /// Shan, Penoncello and Jacobsen (2000).
        R23 = "R23",
        /// Hands and Arp (1981).
        Helium = "Helium",
    }
}

hardcoded! {
    /// A dilute-gas conductivity term hard-coded in CoolProp (its CO₂ Vesovic and `none` terms are dead, ROT-116).
    HardcodedDiluteConductivity {
        /// Huber et al. (2016).
        CarbonDioxideHuberJpcrd2016 = "CarbonDioxideHuberJPCRD2016",
        /// Friend, Ingham and Ely (1991).
        Ethane = "Ethane",
    }
}

hardcoded! {
    /// A critical-enhancement term hard-coded in CoolProp (its Scalabrin CO₂ term is dead, ROT-116).
    HardcodedCriticalConductivity {
        /// Laesecke et al. (1996).
        R123 = "R123",
        /// Tufeu et al. (1984).
        Ammonia = "Ammonia",
        /// No critical term (the four Lemmon blends).
        Absent = "None",
    }
}

/// A fluid's transport models as v8.0.0 ships them: each property's active entry, or none (70 of the 136 fluids have
/// no viscosity model and 73 no conductivity model, map 10 §8.2).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TransportRecord {
    /// Dynamic viscosity.
    pub viscosity: Option<Viscosity>,
    /// Thermal conductivity.
    pub conductivity: Option<Conductivity>,
}

/// A viscosity model with its own source (D14).
#[derive(Clone, Debug, PartialEq)]
pub struct Viscosity {
    /// The active entry's `BibTeX` key, and its DOI when CoolProp's bibliography has one.
    pub source: Source,
    /// Lennard-Jones 12-6 parameters, when the block gives both; otherwise CoolProp estimates both by Chung's method
    /// from the EOS's reducing state (`FluidLibrary.h:1005-1013`; map 05 R17, M8.9).
    pub lennard_jones: Option<LennardJones>,
    /// The form.
    pub model: ViscosityModel,
}

/// Lennard-Jones 12-6 potential parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LennardJones {
    /// Collision diameter σ, m.
    pub sigma: f64,
    /// Well depth over Boltzmann's constant ε/k, K.
    pub epsilon_over_k: f64,
}

/// The viscosity forms of v8.0.0 (map 05 §3.1-§3.2).
#[derive(Clone, Debug, PartialEq)]
pub enum ViscosityModel {
    /// Dilute, initial-density and higher-order stages, evaluated and tested stage by stage (34 fluids; map 05 U3).
    Staged(StagedViscosity),
    /// Extended corresponding states against a reference fluid (15 fluids; map 05 U8).
    Ecs(EcsViscosity),
    /// Bell's residual-entropy scaling, ρ*_sr-CS (7 fluids; map 05 U9).
    RhoSrCs(RhoSrCs),
    /// Chung et al. (1988) (2 fluids; map 05 U9).
    Chung(Chung),
    /// A whole model hard-coded in CoolProp (8 fluids; map 05 U7, U10).
    Hardcoded(HardcodedViscosity),
}

/// The stages of a staged viscosity: η = η₀ + η₁ + Δη (CoolProp has no viscosity critical term outside Water's).
#[derive(Clone, Debug, PartialEq)]
pub struct StagedViscosity {
    /// The dilute-gas term η₀.
    pub dilute: ViscosityDilute,
    /// The initial-density term η₁, where the model has one (20 of 34).
    pub initial: Option<ViscosityInitial>,
    /// The higher-order term Δη.
    pub higher: ViscosityHigher,
}

/// Parallel lists of coefficients and exponents: Σ aᵢ xᵗⁱ in the form's variable x.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Coefficients.
    pub a: Box<[f64]>,
    /// Exponents.
    pub t: Box<[f64]>,
}

impl Series {
    /// A series, its two lists checked to have one length.
    pub fn new(a: Vec<f64>, t: Vec<f64>) -> Result<Series, String> {
        same_lengths(&[("a", &a), ("t", &t)])?;
        Ok(Series { a: a.into(), t: t.into() })
    }
}

/// The dilute-gas viscosity terms (map 05 §3.1).
#[derive(Clone, Debug, PartialEq)]
pub enum ViscosityDilute {
    /// η₀ = C √(M T) / (σ² S*), S* = exp(Σ aᵢ (ln T*)ᵗⁱ).
    CollisionIntegral {
        /// The collision-integral fit in ln T*.
        terms: Series,
        /// Molar mass, kg/mol.
        molar_mass: f64,
        /// The constant C.
        c: f64,
    },
    /// Chapman-Enskog with Neufeld's Ω₂₂ and the Lennard-Jones parameters.
    KineticTheory,
    /// Σ aᵢ Tᵗⁱ.
    PowersOfT(Series),
    /// Σ aᵢ (T/T_r)ᵗⁱ.
    PowersOfTr {
        /// The series in T/T_r.
        terms: Series,
        /// T_r, K.
        t_r: f64,
    },
    /// C √T / Σ aᵢ T*ᵗⁱ.
    CollisionIntegralPowersOfTstar {
        /// The series in T*.
        terms: Series,
        /// T_r, K (CoolProp's `T_reducing`).
        t_r: f64,
        /// The constant C.
        c: f64,
    },
    /// A hard-coded term.
    Hardcoded(HardcodedDiluteViscosity),
}

/// The initial-density viscosity terms.
#[derive(Clone, Debug, PartialEq)]
pub enum ViscosityInitial {
    /// Rainwater-Friend: B_η = N_A σ³ Σ bᵢ T*ᵗⁱ, η₁ = η₀ B_η ρ.
    RainwaterFriend(Series),
    /// Σ nᵢ δᵈⁱ τᵗⁱ in the term's own reducing state.
    Empirical {
        /// Coefficients nᵢ.
        n: Box<[f64]>,
        /// Density exponents dᵢ.
        d: Box<[f64]>,
        /// Temperature exponents tᵢ.
        t: Box<[f64]>,
        /// T_r, K.
        t_r: f64,
        /// ρ_r, mol/m³.
        rho_r: f64,
    },
}

/// The higher-order viscosity terms.
#[derive(Clone, Debug, PartialEq)]
pub enum ViscosityHigher {
    /// The modified Batschinski-Hildebrand form (Vogel 1998, Lemmon and Jacobsen 2004).
    BatschinskiHildebrand(Box<BatschinskiHildebrand>),
    /// Friction theory (Quiñones-Cisneros and Deiters 2006).
    FrictionTheory(Box<FrictionTheory>),
    /// A hard-coded term.
    Hardcoded(HardcodedHigherViscosity),
}

/// Σ aᵢ δᵈ¹ⁱ τᵗ¹ⁱ exp(γᵢ δˡⁱ) + Σ fᵢ δᵈ²ⁱ τᵗ²ⁱ · (1/(δ₀ − δ) − 1/δ₀), δ₀ = Σ gᵢ τʰⁱ / Σ pᵢ τ^qᵢ.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs, reason = "the symbols of the form above, under CoolProp's names")]
pub struct BatschinskiHildebrand {
    /// T_r, K.
    pub t_r: f64,
    /// ρ_r, mol/m³.
    pub rho_r: f64,
    pub a: Box<[f64]>,
    pub t1: Box<[f64]>,
    pub d1: Box<[f64]>,
    pub gamma: Box<[f64]>,
    pub l: Box<[f64]>,
    pub f: Box<[f64]>,
    pub t2: Box<[f64]>,
    pub d2: Box<[f64]>,
    pub g: Box<[f64]>,
    pub h: Box<[f64]>,
    pub p: Box<[f64]>,
    pub q: Box<[f64]>,
}

/// One friction coefficient κ = (A₀ + A₁ψ₁ + A₂ψ₂) τᴺ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kappa {
    /// A₀, A₁, A₂.
    pub a: [f64; 3],
    /// The exponent N of τ.
    pub n: u8,
}

/// The friction-theory term η_f = κ_a p_a + κ_r Δp_r + κ_i p_id + κ_aa p_a² + … (map 05 §3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct FrictionTheory {
    /// T_r of τ = T_r/T, K.
    pub t_r: f64,
    /// ψ₁ = exp(τ) − c₁.
    pub c1: f64,
    /// ψ₂ = exp(τ²) − c₂.
    pub c2: f64,
    /// κ_i's A₀, A₁, A₂ (its exponent of τ is 1).
    pub ki: [f64; 3],
    /// κ_a.
    pub ka: Kappa,
    /// κ_r.
    pub kr: Kappa,
    /// κ_aa.
    pub kaa: Kappa,
    /// κ_rr or κ_drdr: CoolProp takes exactly one.
    pub second: FrictionSecond,
    /// κ_ii, where the model has it.
    pub kii: Option<Kappa>,
    /// κ_rrr and κ_aaa, where the model has them (SF₆).
    pub cubic: Option<FrictionCubic>,
}

/// The second-order repulsive friction coefficient: on p_r² or on Δp_r².
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrictionSecond {
    /// κ_rr, on p_r² (`Arr`).
    Rr(Kappa),
    /// κ_drdr, on Δp_r² (`Adrdr`).
    Drdr(Kappa),
}

/// The third-order friction coefficients.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrictionCubic {
    /// κ_rrr.
    pub krrr: Kappa,
    /// κ_aaa.
    pub kaaa: Kappa,
}

/// ECS viscosity (map 05 §3.2): the reference fluid's background viscosity at the conformal state, the density
/// scaled by ψ = Σ aᵢ (ρ/ρ_r)ᵗⁱ.
#[derive(Clone, Debug, PartialEq)]
pub struct EcsViscosity {
    /// The reference fluid's canonical name (Propane, R134a or Nitrogen in v8.0.0).
    pub reference: Box<str>,
    /// ψ's series in ρ/ρ_r.
    pub psi: Series,
    /// ψ's ρ_r, mol/m³.
    pub psi_rho_r: f64,
}

/// ρ*_sr-CS viscosity (Bell 2016; map 05 §3.2).
#[derive(Clone, Debug, PartialEq)]
pub struct RhoSrCs {
    /// The scaling constant C.
    pub c: f64,
    /// The liquid branch's cubic in x.
    pub c_liq: [f64; 4],
    /// The vapour branch's cubic in x.
    pub c_vap: [f64; 4],
    /// ρ s_r at the critical point, as stored (map 05 R4: R1234yf's is stale against its EOS).
    pub rhosr_critical: f64,
    /// The x where the branches cross (2 in every v8.0.0 file; CoolProp ignores it, map 05 R4).
    pub x_crossover: f64,
}

/// Chung et al. (1988) viscosity, with the constants of its own block, not the EOS's (map 05 §3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chung {
    /// Critical molar density, mol/m³.
    pub rho_c: f64,
    /// Critical temperature, K.
    pub t_c: f64,
    /// Molar mass, kg/mol.
    pub molar_mass: f64,
    /// Dipole moment, Debye.
    pub dipole_moment: f64,
    /// Acentric factor.
    pub acentric: f64,
    /// Association factor κ (0 in both v8.0.0 files; CoolProp hard-codes 0, map 05 §3.2).
    pub kappa: f64,
}

/// A conductivity model with its own source (D14).
#[derive(Clone, Debug, PartialEq)]
pub struct Conductivity {
    /// The block's `BibTeX` key, and its DOI when CoolProp's bibliography has one.
    pub source: Source,
    /// The form.
    pub model: ConductivityModel,
}

/// The conductivity forms of v8.0.0 (map 05 §3.3).
#[derive(Clone, Debug, PartialEq)]
pub enum ConductivityModel {
    /// Dilute, residual and critical stages (40 fluids; map 05 U4).
    Staged(StagedConductivity),
    /// Extended corresponding states against a reference fluid (18 fluids; map 05 U8).
    Ecs(EcsConductivity),
    /// A whole model hard-coded in CoolProp (4 fluids).
    Hardcoded(HardcodedConductivity),
    /// Methane's model, hard-coded in CoolProp, with what it reads of the rest of the fluid (map 05 R10).
    Methane(FriendMethane),
}

/// The stages of a staged conductivity: λ = λ₀ + Δλ_r + Δλ_c (CoolProp needs all three).
#[derive(Clone, Debug, PartialEq)]
pub struct StagedConductivity {
    /// The dilute-gas term.
    pub dilute: ConductivityDilute,
    /// The residual term.
    pub residual: ConductivityResidual,
    /// The critical enhancement.
    pub critical: ConductivityCritical,
    /// The EOS reducing state the forms read: the Olchowy-Sengers term's T_c, ρ_c and p_c, and the τ and δ of
    /// `eta0_and_poly` and `polynomial_and_exponential` (Lemmon and Jacobsen 2004). The default EOS's
    /// `STATES.reducing`, snapshotted at datagen (map 05 R10, ROT-117).
    pub reducing: Reducing,
}

/// A reducing state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reducing {
    /// T, K.
    pub t: f64,
    /// Molar density, mol/m³.
    pub rho: f64,
    /// Pressure, Pa.
    pub p: f64,
}

/// Methane's conductivity: Friend, Ely and Ingham 1989 (dilute, residual and critical terms with the paper's own
/// viscosity), hard-coded in CoolProp (`TransportRoutines.cpp:1063-1140`). What it reads of the rest of the fluid is
/// snapshotted at datagen, so no configuration and no other saturation curve moves it (map 05 R10, ROT-117): CoolProp
/// reads `T_critical()` and `rhomolar_critical()`, which its configuration sets, and a saturation ancillary.
#[derive(Clone, Debug, PartialEq)]
pub struct FriendMethane {
    /// The critical temperature CoolProp's default configuration reads (the superancillary's, `Tcrittrue`), K.
    pub t_c: f64,
    /// The critical density it reads (`rhocrittrue`), mol/m³.
    pub rho_c: f64,
    /// The saturated-vapour density ancillary (`ANCILLARIES.rhoV`), read below the critical point.
    pub rho_v: Ancillary,
    /// The EOS reducing state, whose τ and δ CoolProp's derivatives are taken in.
    pub reducing: Reducing,
}

/// The dilute-gas conductivity terms.
#[derive(Clone, Debug, PartialEq)]
pub enum ConductivityDilute {
    /// Σ Aᵢ T_rⁿⁱ / Σ Bⱼ T_rᵐʲ, T_r = T/T_red.
    RatioOfPolynomials {
        /// The numerator's A and n.
        numerator: Series,
        /// The denominator's B and m.
        denominator: Series,
        /// T_red, K.
        t_r: f64,
    },
    /// A₀ η₀ (µPa s) + Σᵢ≥₁ Aᵢ τᵗⁱ in the EOS's τ (Lemmon and Jacobsen 2004).
    Eta0AndPoly(Series),
    /// A hard-coded term.
    Hardcoded(HardcodedDiluteConductivity),
}

/// The residual conductivity terms.
#[derive(Clone, Debug, PartialEq)]
pub enum ConductivityResidual {
    /// Σ Bᵢ τᵗⁱ δᵈⁱ in the term's own reducing state (ρ_r in mass units).
    Polynomial {
        /// Coefficients Bᵢ.
        b: Box<[f64]>,
        /// Temperature exponents tᵢ.
        t: Box<[f64]>,
        /// Density exponents dᵢ.
        d: Box<[f64]>,
        /// T_r, K.
        t_r: f64,
        /// ρ_r, kg/m³.
        rho_r: f64,
    },
    /// Σ Aᵢ τᵗⁱ δᵈⁱ exp(−γᵢ δˡⁱ) in the EOS's τ and δ.
    PolynomialAndExponential {
        /// Coefficients Aᵢ.
        a: Box<[f64]>,
        /// Temperature exponents tᵢ.
        t: Box<[f64]>,
        /// Density exponents dᵢ.
        d: Box<[f64]>,
        /// γᵢ.
        gamma: Box<[f64]>,
        /// lᵢ.
        l: Box<[f64]>,
    },
}

/// The critical-enhancement terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConductivityCritical {
    /// The simplified Olchowy-Sengers term (map 05 §3.4).
    OlchowySengers(OlchowySengers),
    /// A hard-coded term, or none.
    Hardcoded(HardcodedCriticalConductivity),
}

/// The simplified Olchowy-Sengers constants; CoolProp's defaults fill what a block omits (`CoolPropFluid.h:179-193`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OlchowySengers {
    /// The cutoff wavenumber q_D, 1/m.
    pub q_d: f64,
    /// ξ₀, m.
    pub zeta0: f64,
    /// Γ (CoolProp's `GAMMA`).
    pub big_gamma: f64,
    /// γ.
    pub gamma: f64,
    /// R_D (CoolProp's `R0`).
    pub r0: f64,
    /// ν (CoolProp reads no `nu` and fixes 0.63; CarbonDioxide's block writes 0.63).
    pub nu: f64,
    /// The reference temperature, K; `None`: 1.5 T_c of the EOS's reducing state.
    pub t_ref: Option<f64>,
}

impl OlchowySengers {
    /// CoolProp's defaults, `T_ref` aside.
    pub const COOLPROP: OlchowySengers =
        OlchowySengers { q_d: 2e9, zeta0: 1.94e-10, big_gamma: 0.0496, gamma: 1.239, r0: 1.03, nu: 0.63, t_ref: None };
}

/// ECS conductivity (McLinden, Klein and Perkins 2000; map 05 §3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct EcsConductivity {
    /// The reference fluid's canonical name.
    pub reference: Box<str>,
    /// ψ's series in ρ/ρ_r.
    pub psi: Series,
    /// ψ's ρ_r, mol/m³.
    pub psi_rho_r: f64,
    /// f_int's series in T/T_r.
    pub f_int: Series,
    /// f_int's T_r, K.
    pub f_int_t_r: f64,
    /// The critical term's cutoff wavenumber q_D, 1/m (CoolProp never reads it, map 05 R2; read from M8.8).
    pub q_d: f64,
    /// The EOS reducing state the critical term reads, as `StagedConductivity::reducing` (ROT-117).
    pub reducing: Reducing,
}

/// Writes a transport record field by field, each with its name. The blob encoder writes the values in order and the
/// dump writes them under their names, from one description per type ([`Part`]).
pub trait Put {
    /// A number.
    fn num(&mut self, name: &'static str, x: f64);
    /// A list of numbers.
    fn nums(&mut self, name: &'static str, xs: &[f64]);
    /// A small integer (an exponent).
    fn small(&mut self, name: &'static str, x: u8);
    /// A string.
    fn text(&mut self, name: &'static str, x: &str);
    /// One of `names`, by its position: a variant, or a hardcoded model.
    fn choice(&mut self, name: &'static str, names: &'static [&'static str], index: usize);
    /// Whether the optional field or part `name` follows.
    fn present(&mut self, name: &'static str, present: bool);
    /// Opens the part `name`; its fields follow until [`Put::close`].
    fn open(&mut self, name: &'static str);
    /// Closes the innermost open part.
    fn close(&mut self);
}

/// Reads what [`Put`] wrote, in the same order.
pub trait Take {
    /// A number.
    fn num(&mut self, name: &'static str) -> Result<f64, String>;
    /// A list of numbers.
    fn nums(&mut self, name: &'static str) -> Result<Vec<f64>, String>;
    /// A small integer.
    fn small(&mut self, name: &'static str) -> Result<u8, String>;
    /// A string.
    fn text(&mut self, name: &'static str) -> Result<String, String>;
    /// The position in `names` of the one written.
    fn choice(&mut self, name: &'static str, names: &'static [&'static str]) -> Result<usize, String>;
    /// Whether the optional field or part `name` follows.
    fn present(&mut self, name: &'static str) -> Result<bool, String>;
    /// Enters the part `name`.
    fn open(&mut self, name: &'static str) -> Result<(), String>;
    /// Leaves the innermost part.
    fn close(&mut self) -> Result<(), String>;
}

/// One type of the transport record, described once for [`Put`] and [`Take`].
pub trait Part: Sized {
    /// Writes the fields.
    fn put(&self, p: &mut dyn Put);
    /// Reads them back, validated.
    fn take(t: &mut dyn Take) -> Result<Self, String>;
}

/// Every list has the length of the first.
fn same_lengths(lists: &[(&str, &[f64])]) -> Result<(), String> {
    let Some((first, xs)) = lists.first() else { return Ok(()) };
    match lists.iter().find(|(_, ys)| ys.len() != xs.len()) {
        Some((name, ys)) => Err(format!("{name} has {} entries, {first} has {}", ys.len(), xs.len())),
        None => Ok(()),
    }
}

fn part(p: &mut dyn Put, name: &'static str, x: &impl Part) {
    p.open(name);
    x.put(p);
    p.close();
}

fn take_part<T: Part>(t: &mut dyn Take, name: &'static str) -> Result<T, String> {
    t.open(name)?;
    let x = T::take(t).map_err(|e| format!("{name}: {e}"))?;
    t.close()?;
    Ok(x)
}

fn opt_part(p: &mut dyn Put, name: &'static str, x: Option<&impl Part>) {
    p.present(name, x.is_some());
    if let Some(x) = x {
        part(p, name, x);
    }
}

fn take_opt_part<T: Part>(t: &mut dyn Take, name: &'static str) -> Result<Option<T>, String> {
    if t.present(name)? { take_part(t, name).map(Some) } else { Ok(None) }
}

/// A list of exactly `N` numbers.
fn fixed<const N: usize>(t: &mut dyn Take, name: &'static str) -> Result<[f64; N], String> {
    let xs = t.nums(name)?;
    let len = xs.len();
    <[f64; N]>::try_from(xs).map_err(|_| format!("{name} has {len} entries, not {N}"))
}

/// The lists `names` of one block, checked to have one length.
fn parallel<const N: usize>(t: &mut dyn Take, names: [&'static str; N]) -> Result<[Box<[f64]>; N], String> {
    let mut lists: [Box<[f64]>; N] = core::array::from_fn(|_| Box::default());
    for (list, name) in lists.iter_mut().zip(names) {
        *list = t.nums(name)?.into();
    }
    let named: Vec<(&str, &[f64])> = names.iter().copied().zip(lists.iter().map(|l| &l[..])).collect();
    same_lengths(&named)?;
    Ok(lists)
}

/// A variant: its position in `kinds`, then its fields.
fn variant(p: &mut dyn Put, kinds: &'static [&'static str], index: usize, fields: &impl Part) {
    p.choice("kind", kinds, index);
    fields.put(p);
}

impl<T: Part> Part for Box<T> {
    fn put(&self, p: &mut dyn Put) {
        (**self).put(p);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        T::take(t).map(Box::new)
    }
}

fn put_lists(p: &mut dyn Put, lists: &[(&'static str, &[f64])]) {
    lists.iter().for_each(|(name, xs)| p.nums(name, xs));
}

impl Part for Series {
    fn put(&self, p: &mut dyn Put) {
        put_lists(p, &[("a", &self.a), ("t", &self.t)]);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let [a, t] = parallel(t, ["a", "t"])?;
        Ok(Series { a, t })
    }
}

/// A source as transport carries it: the key and its DOI (published terms, no further citations).
impl Part for Source {
    fn put(&self, p: &mut dyn Put) {
        p.text("bibkey", &self.bibkey);
        p.present("doi", self.doi.is_some());
        if let Some(doi) = &self.doi {
            p.text("doi", doi);
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let bibkey = t.text("bibkey")?;
        let doi = if t.present("doi")? { Some(t.text("doi")?) } else { None };
        Ok(Source::new(&bibkey, doi.as_deref(), DataTerms::Published))
    }
}

impl Part for TransportRecord {
    fn put(&self, p: &mut dyn Put) {
        opt_part(p, "viscosity", self.viscosity.as_ref());
        opt_part(p, "conductivity", self.conductivity.as_ref());
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(TransportRecord {
            viscosity: take_opt_part(t, "viscosity")?,
            conductivity: take_opt_part(t, "conductivity")?,
        })
    }
}

impl Part for Viscosity {
    fn put(&self, p: &mut dyn Put) {
        part(p, "source", &self.source);
        opt_part(p, "lennard_jones", self.lennard_jones.as_ref());
        part(p, "model", &self.model);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(Viscosity {
            source: take_part(t, "source")?,
            lennard_jones: take_opt_part(t, "lennard_jones")?,
            model: take_part(t, "model")?,
        })
    }
}

impl Part for LennardJones {
    fn put(&self, p: &mut dyn Put) {
        p.num("sigma", self.sigma);
        p.num("epsilon_over_k", self.epsilon_over_k);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(LennardJones { sigma: t.num("sigma")?, epsilon_over_k: t.num("epsilon_over_k")? })
    }
}

const VISCOSITY_KINDS: &[&str] = &["staged", "ECS", "rhosr-CS", "Chung", "hardcoded"];

impl Part for ViscosityModel {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ViscosityModel::Staged(x) => variant(p, VISCOSITY_KINDS, 0, x),
            ViscosityModel::Ecs(x) => variant(p, VISCOSITY_KINDS, 1, x),
            ViscosityModel::RhoSrCs(x) => variant(p, VISCOSITY_KINDS, 2, x),
            ViscosityModel::Chung(x) => variant(p, VISCOSITY_KINDS, 3, x),
            ViscosityModel::Hardcoded(x) => variant(p, VISCOSITY_KINDS, 4, x),
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(match t.choice("kind", VISCOSITY_KINDS)? {
            0 => ViscosityModel::Staged(StagedViscosity::take(t)?),
            1 => ViscosityModel::Ecs(EcsViscosity::take(t)?),
            2 => ViscosityModel::RhoSrCs(RhoSrCs::take(t)?),
            3 => ViscosityModel::Chung(Chung::take(t)?),
            _ => ViscosityModel::Hardcoded(HardcodedViscosity::take(t)?),
        })
    }
}

impl Part for StagedViscosity {
    fn put(&self, p: &mut dyn Put) {
        part(p, "dilute", &self.dilute);
        opt_part(p, "initial_density", self.initial.as_ref());
        part(p, "higher_order", &self.higher);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(StagedViscosity {
            dilute: take_part(t, "dilute")?,
            initial: take_opt_part(t, "initial_density")?,
            higher: take_part(t, "higher_order")?,
        })
    }
}

const DILUTE_VISCOSITY_KINDS: &[&str] = &[
    "collision_integral",
    "kinetic_theory",
    "powers_of_T",
    "powers_of_Tr",
    "collision_integral_powers_of_Tstar",
    "hardcoded",
];

impl Part for ViscosityDilute {
    fn put(&self, p: &mut dyn Put) {
        let (i, terms, nums): (usize, Option<&Series>, &[(&'static str, f64)]) = match self {
            ViscosityDilute::CollisionIntegral { terms, molar_mass, c } => {
                (0, Some(terms), &[("molar_mass", *molar_mass), ("C", *c)])
            }
            ViscosityDilute::KineticTheory => (1, None, &[]),
            ViscosityDilute::PowersOfT(terms) => (2, Some(terms), &[]),
            ViscosityDilute::PowersOfTr { terms, t_r } => (3, Some(terms), &[("T_reducing", *t_r)]),
            ViscosityDilute::CollisionIntegralPowersOfTstar { terms, t_r, c } => {
                (4, Some(terms), &[("T_reducing", *t_r), ("C", *c)])
            }
            ViscosityDilute::Hardcoded(x) => return variant(p, DILUTE_VISCOSITY_KINDS, 5, x),
        };
        p.choice("kind", DILUTE_VISCOSITY_KINDS, i);
        terms.into_iter().for_each(|terms| terms.put(p));
        nums.iter().for_each(|(name, x)| p.num(name, *x));
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(match t.choice("kind", DILUTE_VISCOSITY_KINDS)? {
            0 => ViscosityDilute::CollisionIntegral {
                terms: Series::take(t)?,
                molar_mass: t.num("molar_mass")?,
                c: t.num("C")?,
            },
            1 => ViscosityDilute::KineticTheory,
            2 => ViscosityDilute::PowersOfT(Series::take(t)?),
            3 => ViscosityDilute::PowersOfTr { terms: Series::take(t)?, t_r: t.num("T_reducing")? },
            4 => ViscosityDilute::CollisionIntegralPowersOfTstar {
                terms: Series::take(t)?,
                t_r: t.num("T_reducing")?,
                c: t.num("C")?,
            },
            _ => ViscosityDilute::Hardcoded(HardcodedDiluteViscosity::take(t)?),
        })
    }
}

const INITIAL_KINDS: &[&str] = &["Rainwater-Friend", "empirical"];

impl Part for ViscosityInitial {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ViscosityInitial::RainwaterFriend(terms) => variant(p, INITIAL_KINDS, 0, terms),
            ViscosityInitial::Empirical { n, d, t, t_r, rho_r } => {
                p.choice("kind", INITIAL_KINDS, 1);
                put_lists(p, &[("n", n), ("d", d), ("t", t)]);
                p.num("T_reducing", *t_r);
                p.num("rhomolar_reducing", *rho_r);
            }
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        if t.choice("kind", INITIAL_KINDS)? == 0 {
            return Ok(ViscosityInitial::RainwaterFriend(Series::take(t)?));
        }
        let [n, d, ts] = parallel(t, ["n", "d", "t"])?;
        Ok(ViscosityInitial::Empirical { n, d, t: ts, t_r: t.num("T_reducing")?, rho_r: t.num("rhomolar_reducing")? })
    }
}

const HIGHER_KINDS: &[&str] = &["modified_Batschinski_Hildebrand", "friction_theory", "hardcoded"];

impl Part for ViscosityHigher {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ViscosityHigher::BatschinskiHildebrand(x) => variant(p, HIGHER_KINDS, 0, x),
            ViscosityHigher::FrictionTheory(x) => variant(p, HIGHER_KINDS, 1, x),
            ViscosityHigher::Hardcoded(x) => variant(p, HIGHER_KINDS, 2, x),
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(match t.choice("kind", HIGHER_KINDS)? {
            0 => ViscosityHigher::BatschinskiHildebrand(Part::take(t)?),
            1 => ViscosityHigher::FrictionTheory(Part::take(t)?),
            _ => ViscosityHigher::Hardcoded(HardcodedHigherViscosity::take(t)?),
        })
    }
}

impl Part for BatschinskiHildebrand {
    fn put(&self, p: &mut dyn Put) {
        p.num("T_reduce", self.t_r);
        p.num("rhomolar_reduce", self.rho_r);
        let first = [("a", &self.a), ("t1", &self.t1), ("d1", &self.d1), ("gamma", &self.gamma), ("l", &self.l)];
        let second = [("f", &self.f), ("t2", &self.t2), ("d2", &self.d2)];
        let delta0 = [("g", &self.g), ("h", &self.h), ("p", &self.p), ("q", &self.q)];
        for (name, xs) in first.into_iter().chain(second).chain(delta0) {
            p.nums(name, xs);
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let (t_r, rho_r) = (t.num("T_reduce")?, t.num("rhomolar_reduce")?);
        let [a, t1, d1, gamma, l] = parallel(t, ["a", "t1", "d1", "gamma", "l"])?;
        let [f, t2, d2] = parallel(t, ["f", "t2", "d2"])?;
        let [g, h] = parallel(t, ["g", "h"])?;
        let [p, q] = parallel(t, ["p", "q"])?;
        Ok(BatschinskiHildebrand { t_r, rho_r, a, t1, d1, gamma, l, f, t2, d2, g, h, p, q })
    }
}

impl Part for Kappa {
    fn put(&self, p: &mut dyn Put) {
        p.nums("A", &self.a);
        p.small("N", self.n);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(Kappa { a: fixed(t, "A")?, n: t.small("N")? })
    }
}

const SECOND_KINDS: &[&str] = &["Arr", "Adrdr"];

impl Part for FrictionTheory {
    fn put(&self, p: &mut dyn Put) {
        [("T_reduce", self.t_r), ("c1", self.c1), ("c2", self.c2)].into_iter().for_each(|(name, x)| p.num(name, x));
        p.nums("Ai", &self.ki);
        part(p, "a", &self.ka);
        part(p, "r", &self.kr);
        part(p, "aa", &self.kaa);
        let (i, second) = match &self.second {
            FrictionSecond::Rr(k) => (0, k),
            FrictionSecond::Drdr(k) => (1, k),
        };
        p.choice("second", SECOND_KINDS, i);
        part(p, "second_order", second);
        opt_part(p, "ii", self.kii.as_ref());
        opt_part(p, "cubic", self.cubic.as_ref());
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let (t_r, c1, c2, ki) = (t.num("T_reduce")?, t.num("c1")?, t.num("c2")?, fixed(t, "Ai")?);
        let (ka, kr, kaa) = (take_part(t, "a")?, take_part(t, "r")?, take_part(t, "aa")?);
        let rr = t.choice("second", SECOND_KINDS)? == 0;
        let second = take_part(t, "second_order")?;
        let second = if rr { FrictionSecond::Rr(second) } else { FrictionSecond::Drdr(second) };
        let (kii, cubic) = (take_opt_part(t, "ii")?, take_opt_part(t, "cubic")?);
        Ok(FrictionTheory { t_r, c1, c2, ki, ka, kr, kaa, second, kii, cubic })
    }
}

impl Part for FrictionCubic {
    fn put(&self, p: &mut dyn Put) {
        part(p, "rrr", &self.krrr);
        part(p, "aaa", &self.kaaa);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(FrictionCubic { krrr: take_part(t, "rrr")?, kaaa: take_part(t, "aaa")? })
    }
}

impl Part for EcsViscosity {
    fn put(&self, p: &mut dyn Put) {
        p.text("reference_fluid", &self.reference);
        part(p, "psi", &self.psi);
        p.num("psi_rhomolar_reducing", self.psi_rho_r);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(EcsViscosity {
            reference: t.text("reference_fluid")?.into(),
            psi: take_part(t, "psi")?,
            psi_rho_r: t.num("psi_rhomolar_reducing")?,
        })
    }
}

impl Part for RhoSrCs {
    fn put(&self, p: &mut dyn Put) {
        p.num("C", self.c);
        p.nums("c_liq", &self.c_liq);
        p.nums("c_vap", &self.c_vap);
        p.num("rhosr_critical", self.rhosr_critical);
        p.num("x_crossover", self.x_crossover);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(RhoSrCs {
            c: t.num("C")?,
            c_liq: fixed(t, "c_liq")?,
            c_vap: fixed(t, "c_vap")?,
            rhosr_critical: t.num("rhosr_critical")?,
            x_crossover: t.num("x_crossover")?,
        })
    }
}

const CHUNG_FIELDS: [&str; 6] =
    ["rhomolar_critical", "T_critical", "molar_mass", "dipole_moment_D", "acentric", "kappa"];

impl Part for Chung {
    fn put(&self, p: &mut dyn Put) {
        let values = [self.rho_c, self.t_c, self.molar_mass, self.dipole_moment, self.acentric, self.kappa];
        CHUNG_FIELDS.into_iter().zip(values).for_each(|(name, x)| p.num(name, x));
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let mut v = [0.0; 6];
        for (x, name) in v.iter_mut().zip(CHUNG_FIELDS) {
            *x = t.num(name)?;
        }
        let [rho_c, t_c, molar_mass, dipole_moment, acentric, kappa] = v;
        Ok(Chung { rho_c, t_c, molar_mass, dipole_moment, acentric, kappa })
    }
}

impl Part for Conductivity {
    fn put(&self, p: &mut dyn Put) {
        part(p, "source", &self.source);
        part(p, "model", &self.model);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(Conductivity { source: take_part(t, "source")?, model: take_part(t, "model")? })
    }
}

const CONDUCTIVITY_KINDS: &[&str] = &["staged", "ECS", "hardcoded", "Friend_methane"];

impl Part for ConductivityModel {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ConductivityModel::Staged(x) => variant(p, CONDUCTIVITY_KINDS, 0, x),
            ConductivityModel::Ecs(x) => variant(p, CONDUCTIVITY_KINDS, 1, x),
            ConductivityModel::Hardcoded(x) => variant(p, CONDUCTIVITY_KINDS, 2, x),
            ConductivityModel::Methane(x) => variant(p, CONDUCTIVITY_KINDS, 3, x),
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(match t.choice("kind", CONDUCTIVITY_KINDS)? {
            0 => ConductivityModel::Staged(StagedConductivity::take(t)?),
            1 => ConductivityModel::Ecs(EcsConductivity::take(t)?),
            2 => ConductivityModel::Hardcoded(HardcodedConductivity::take(t)?),
            _ => ConductivityModel::Methane(FriendMethane::take(t)?),
        })
    }
}

impl Part for Reducing {
    fn put(&self, p: &mut dyn Put) {
        p.num("T", self.t);
        p.num("rhomolar", self.rho);
        p.num("p", self.p);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(Reducing { t: t.num("T")?, rho: t.num("rhomolar")?, p: t.num("p")? })
    }
}

impl Part for FriendMethane {
    fn put(&self, p: &mut dyn Put) {
        p.num("T_critical", self.t_c);
        p.num("rhomolar_critical", self.rho_c);
        part(p, "rhoV", &self.rho_v);
        part(p, "reducing", &self.reducing);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(FriendMethane {
            t_c: t.num("T_critical")?,
            rho_c: t.num("rhomolar_critical")?,
            rho_v: take_part(t, "rhoV")?,
            reducing: take_part(t, "reducing")?,
        })
    }
}

impl Part for Ancillary {
    fn put(&self, p: &mut dyn Put) {
        p.small("exponential", u8::from(self.exponential));
        p.small("using_tau_r", u8::from(self.tau_r));
        p.num("T_r", self.t_r);
        p.num("reducing_value", self.reducing);
        put_lists(p, &[("n", &self.n), ("t", &self.t)]);
        p.num("Tmin", self.t_min);
        p.num("Tmax", self.t_max);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        let flag = |t: &mut dyn Take, name| match t.small(name)? {
            0 => Ok(false),
            1 => Ok(true),
            x => Err(format!("{name}: {x} is not a flag")),
        };
        let (exponential, tau_r) = (flag(t, "exponential")?, flag(t, "using_tau_r")?);
        let (t_r, reducing) = (t.num("T_r")?, t.num("reducing_value")?);
        let [n, ts] = parallel(t, ["n", "t"])?;
        let (t_min, t_max) = (t.num("Tmin")?, t.num("Tmax")?);
        Ok(Ancillary { exponential, tau_r, t_r, reducing, n: n.into(), t: ts.into(), t_min, t_max })
    }
}

impl Part for StagedConductivity {
    fn put(&self, p: &mut dyn Put) {
        part(p, "dilute", &self.dilute);
        part(p, "residual", &self.residual);
        part(p, "critical", &self.critical);
        part(p, "reducing", &self.reducing);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(StagedConductivity {
            dilute: take_part(t, "dilute")?,
            residual: take_part(t, "residual")?,
            critical: take_part(t, "critical")?,
            reducing: take_part(t, "reducing")?,
        })
    }
}

const DILUTE_CONDUCTIVITY_KINDS: &[&str] = &["ratio_of_polynomials", "eta0_and_poly", "hardcoded"];

impl Part for ConductivityDilute {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ConductivityDilute::RatioOfPolynomials { numerator, denominator, t_r } => {
                p.choice("kind", DILUTE_CONDUCTIVITY_KINDS, 0);
                part(p, "numerator", numerator);
                part(p, "denominator", denominator);
                p.num("T_reducing", *t_r);
            }
            ConductivityDilute::Eta0AndPoly(terms) => variant(p, DILUTE_CONDUCTIVITY_KINDS, 1, terms),
            ConductivityDilute::Hardcoded(x) => variant(p, DILUTE_CONDUCTIVITY_KINDS, 2, x),
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(match t.choice("kind", DILUTE_CONDUCTIVITY_KINDS)? {
            0 => ConductivityDilute::RatioOfPolynomials {
                numerator: take_part(t, "numerator")?,
                denominator: take_part(t, "denominator")?,
                t_r: t.num("T_reducing")?,
            },
            1 => ConductivityDilute::Eta0AndPoly(Series::take(t)?),
            _ => ConductivityDilute::Hardcoded(HardcodedDiluteConductivity::take(t)?),
        })
    }
}

const RESIDUAL_KINDS: &[&str] = &["polynomial", "polynomial_and_exponential"];

impl Part for ConductivityResidual {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ConductivityResidual::Polynomial { b, t, d, t_r, rho_r } => {
                p.choice("kind", RESIDUAL_KINDS, 0);
                put_lists(p, &[("B", b), ("t", t), ("d", d)]);
                p.num("T_reducing", *t_r);
                p.num("rhomass_reducing", *rho_r);
            }
            ConductivityResidual::PolynomialAndExponential { a, t, d, gamma, l } => {
                p.choice("kind", RESIDUAL_KINDS, 1);
                put_lists(p, &[("A", a), ("t", t), ("d", d), ("gamma", gamma), ("l", l)]);
            }
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        if t.choice("kind", RESIDUAL_KINDS)? == 0 {
            let [b, ts, d] = parallel(t, ["B", "t", "d"])?;
            let (t_r, rho_r) = (t.num("T_reducing")?, t.num("rhomass_reducing")?);
            return Ok(ConductivityResidual::Polynomial { b, t: ts, d, t_r, rho_r });
        }
        let [a, ts, d, gamma, l] = parallel(t, ["A", "t", "d", "gamma", "l"])?;
        Ok(ConductivityResidual::PolynomialAndExponential { a, t: ts, d, gamma, l })
    }
}

const CRITICAL_KINDS: &[&str] = &["simplified_Olchowy_Sengers", "hardcoded"];
const OLCHOWY_SENGERS_FIELDS: [&str; 6] = ["qD", "zeta0", "GAMMA", "gamma", "R0", "nu"];

impl Part for ConductivityCritical {
    fn put(&self, p: &mut dyn Put) {
        match self {
            ConductivityCritical::OlchowySengers(os) => {
                p.choice("kind", CRITICAL_KINDS, 0);
                let values = [os.q_d, os.zeta0, os.big_gamma, os.gamma, os.r0, os.nu];
                OLCHOWY_SENGERS_FIELDS.into_iter().zip(values).for_each(|(name, x)| p.num(name, x));
                p.present("T_ref", os.t_ref.is_some());
                if let Some(t_ref) = os.t_ref {
                    p.num("T_ref", t_ref);
                }
            }
            ConductivityCritical::Hardcoded(x) => variant(p, CRITICAL_KINDS, 1, x),
        }
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        if t.choice("kind", CRITICAL_KINDS)? == 1 {
            return Ok(ConductivityCritical::Hardcoded(HardcodedCriticalConductivity::take(t)?));
        }
        let mut v = [0.0; 6];
        for (x, name) in v.iter_mut().zip(OLCHOWY_SENGERS_FIELDS) {
            *x = t.num(name)?;
        }
        let [q_d, zeta0, big_gamma, gamma, r0, nu] = v;
        let t_ref = if t.present("T_ref")? { Some(t.num("T_ref")?) } else { None };
        Ok(ConductivityCritical::OlchowySengers(OlchowySengers { q_d, zeta0, big_gamma, gamma, r0, nu, t_ref }))
    }
}

impl Part for EcsConductivity {
    fn put(&self, p: &mut dyn Put) {
        p.text("reference_fluid", &self.reference);
        part(p, "psi", &self.psi);
        p.num("psi_rhomolar_reducing", self.psi_rho_r);
        part(p, "f_int", &self.f_int);
        p.num("f_int_T_reducing", self.f_int_t_r);
        p.num("q_D", self.q_d);
        part(p, "reducing", &self.reducing);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(EcsConductivity {
            reference: t.text("reference_fluid")?.into(),
            psi: take_part(t, "psi")?,
            psi_rho_r: t.num("psi_rhomolar_reducing")?,
            f_int: take_part(t, "f_int")?,
            f_int_t_r: t.num("f_int_T_reducing")?,
            q_d: t.num("q_D")?,
            reducing: take_part(t, "reducing")?,
        })
    }
}

impl Viscosity {
    /// The stages at `x` of a staged model, an ECS model against one of `references` or an IAPWS release; another
    /// form says the step that evaluates it.
    pub(crate) fn stages(&self, x: &TransportInputs<'_>, references: &[Fluid]) -> Result<ViscosityStages, Error> {
        match &self.model {
            ViscosityModel::Staged(staged) => viscosity::stages(staged, self.lennard_jones, x),
            ViscosityModel::Ecs(model) => ecs::viscosity(model, self.lennard_jones, x, references),
            ViscosityModel::RhoSrCs(_) | ViscosityModel::Chung(_) => {
                Err(lands("evaluating this viscosity model", "M8.9"))
            }
            ViscosityModel::Hardcoded(HardcodedViscosity::Water) => Ok(iapws::water_viscosity_stages(x)),
            ViscosityModel::Hardcoded(HardcodedViscosity::HeavyWater) => Ok(iapws::heavy_water_viscosity(x)),
            ViscosityModel::Hardcoded(h) => Err(lands(&format!("evaluating the {} viscosity", h.name()), "M8.9")),
        }
    }
}

impl Conductivity {
    /// The stages at `x` of a staged model, Methane's, an ECS model against one of `references` or an IAPWS release;
    /// `eta` is the fluid's own viscosity there, read only by the forms that need it (map 05 R6), and `lj` its
    /// viscosity block's Lennard-Jones parameters, ECS's dilute η. Another form says the step that evaluates it.
    pub(crate) fn stages(
        &self,
        x: &TransportInputs<'_>,
        eta: &Result<ViscosityStages, Error>,
        lj: Option<LennardJones>,
        references: &[Fluid],
    ) -> Result<ConductivityStages, Error> {
        match &self.model {
            ConductivityModel::Staged(staged) => conductivity::stages(staged, x, eta),
            ConductivityModel::Methane(methane) => conductivity::friend_methane(methane, x),
            ConductivityModel::Ecs(model) => ecs::conductivity(model, lj, x, eta, references),
            ConductivityModel::Hardcoded(HardcodedConductivity::Water) => iapws::water_conductivity_stages(x, eta),
            ConductivityModel::Hardcoded(HardcodedConductivity::HeavyWater) => Ok(iapws::heavy_water_conductivity(x)),
            ConductivityModel::Hardcoded(h) => Err(lands(&format!("evaluating the {} conductivity", h.name()), "M8.9")),
        }
    }
}

impl TransportRecord {
    /// Applies one edit of the corrections overlay (E15); an edit of another part of the record is not this one's.
    pub fn apply(&mut self, edit: Edit) -> Result<(), LoadError> {
        let missing = |what: &str| LoadError::Format(format!("no {what} to edit").into());
        match edit {
            Edit::EcsQd(q_d) => match self.conductivity.as_mut().map(|c| &mut c.model) {
                Some(ConductivityModel::Ecs(ecs)) => ecs.q_d = q_d,
                _ => return Err(missing("ECS conductivity")),
            },
            Edit::RhosrCritical(x) => match self.viscosity.as_mut().map(|v| &mut v.model) {
                Some(ViscosityModel::RhoSrCs(rhosr)) => rhosr.rhosr_critical = x,
                _ => return Err(missing("rhosr-CS viscosity")),
            },
            _ => {}
        }
        Ok(())
    }

    /// The record under `set`: `Corrected` applies the transport edits among `patches` (E15), `Parity` none. The
    /// registry's transport closure calls this on the decoded section.
    pub(crate) fn corrected(mut self, patches: &[Patch], set: DataSet) -> Result<Self, LoadError> {
        if set == DataSet::Corrected {
            patches.iter().try_for_each(|p| self.apply(p.edit))?;
        }
        Ok(self)
    }

    /// The canonical names of the ECS reference fluids the models name, viscosity's first.
    pub fn references(&self) -> Vec<&str> {
        let viscosity = self.viscosity.as_ref().and_then(|v| match &v.model {
            ViscosityModel::Ecs(ecs) => Some(&*ecs.reference),
            _ => None,
        });
        let conductivity = self.conductivity.as_ref().and_then(|c| match &c.model {
            ConductivityModel::Ecs(ecs) => Some(&*ecs.reference),
            _ => None,
        });
        viscosity.into_iter().chain(conductivity).collect()
    }
}

/// A surface-tension fit σ(T) = Σ aᵢ (1 − T/T_c)ⁿⁱ (108 fluids; map 05 U5), with the T_c it was fitted with (Mulero's
/// own, not the EOS's; map 13 §3) and its own source (D14).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceTension {
    /// The fit's `BibTeX` key, and its DOI when CoolProp's bibliography has one.
    pub source: Source,
    /// The critical temperature the fit was made with, K.
    pub t_c: f64,
    /// The coefficients aᵢ (`a`) and exponents nᵢ (`t`).
    pub terms: Series,
}

impl SurfaceTension {
    /// σ in N/m at `t`, over the fit's range: from `t_min`, the fluid's, to the lower of the fit's own T_c and `t_c_eos`,
    /// the model's critical temperature, above which there is no interface. A fit is never evaluated outside its range
    /// (user decision 6; ROT-114): CoolProp throws for T_c,σ < T ≤ T_c,EOS (map 05 R13), and map 05 Q4's σ = 0 there is
    /// not taken.
    pub(crate) fn eval(&self, t: f64, t_min: f64, t_c_eos: f64) -> Result<f64, Error> {
        if !t.is_finite() {
            return Err(Error::InvalidInput { quantity: "T", value: t });
        }
        if t < t_min {
            return Err(DomainError::BelowMinTemperature { t, t_min }.into());
        }
        let t_max = self.t_c.min(t_c_eos);
        if t > t_max {
            return Err(DomainError::AboveMaxTemperature { t, t_max }.into());
        }
        let x = 1.0 - t / self.t_c;
        Ok(self.terms.a.iter().zip(&self.terms.t).map(|(a, n)| a * math::powf(x, *n)).sum())
    }
}

impl Part for SurfaceTension {
    fn put(&self, p: &mut dyn Put) {
        part(p, "source", &self.source);
        p.num("Tc", self.t_c);
        part(p, "terms", &self.terms);
    }
    fn take(t: &mut dyn Take) -> Result<Self, String> {
        Ok(SurfaceTension { source: take_part(t, "source")?, t_c: t.num("Tc")?, terms: take_part(t, "terms")? })
    }
}

/// η's and λ's stages at one state, each or why there is none.
pub(crate) type Stages = (Result<ViscosityStages, Error>, Result<ConductivityStages, Error>);

/// The transport closure of one fluid, materialised on its first transport call: its decoded models, the corrections
/// overlay applied, and strong handles to the reference fluids the registry resolved when it built the layer (E5), so
/// they work even after the registry is gone (S-05).
#[derive(Clone, Debug, Default)]
pub(crate) struct TransportSet {
    pub(crate) record: TransportRecord,
    pub(crate) surface_tension: Option<SurfaceTension>,
    pub(crate) references: Box<[Fluid]>,
}

impl TransportSet {
    /// η at `x`, or `NoModel`.
    pub(crate) fn viscosity(&self, x: &TransportInputs<'_>) -> Result<f64, Error> {
        let viscosity = self.record.viscosity.as_ref().ok_or(Error::NoModel { prop: Prop::Viscosity })?;
        viscosity.stages(x, &self.references).map(|s| s.total())
    }

    /// λ at `x`, or `NoModel`.
    pub(crate) fn conductivity(&self, x: &TransportInputs<'_>) -> Result<f64, Error> {
        self.evaluate(x).1.map(|s| s.total())
    }

    /// η's and λ's stages at `x`, η evaluated once and passed to λ explicitly (map 05 R6, U6).
    pub(crate) fn evaluate(&self, x: &TransportInputs<'_>) -> Stages {
        let viscosity = self.record.viscosity.as_ref().ok_or(Error::NoModel { prop: Prop::Viscosity });
        let eta = viscosity.and_then(|v| v.stages(x, &self.references));
        let conductivity = self.record.conductivity.as_ref().ok_or(Error::NoModel { prop: Prop::Conductivity });
        let lj = self.record.viscosity.as_ref().and_then(|v| v.lennard_jones);
        let lambda = conductivity.and_then(|c| c.stages(x, &eta, lj, &self.references));
        (eta, lambda)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::data::{FluidRecord, Patch};

    fn series(n: usize) -> Series {
        Series::new((0..n).map(|i| 1.5 + i as f64).collect(), (0..n).map(|i| -0.25 * i as f64).collect()).unwrap()
    }

    fn lists<const N: usize>(n: usize) -> [Box<[f64]>; N] {
        core::array::from_fn(|k| (0..n).map(|i| (k * 10 + i) as f64 + 0.5).collect())
    }

    fn kappa(n: u8) -> Kappa {
        Kappa { a: [1.0e-6, -2.0e-7, 3.5e-8], n }
    }

    fn source(key: &str, doi: Option<&str>) -> Source {
        Source::new(key, doi, DataTerms::Published)
    }

    fn bh() -> ViscosityHigher {
        let [a, t1, d1, gamma, l] = lists(3);
        let [f, t2, d2] = lists(2);
        let [g, h, p, q] = lists(1);
        ViscosityHigher::BatschinskiHildebrand(Box::new(BatschinskiHildebrand {
            t_r: 369.825,
            rho_r: 5000.0,
            a,
            t1,
            d1,
            gamma,
            l,
            f,
            t2,
            d2,
            g,
            h,
            p,
            q,
        }))
    }

    fn friction(second: FrictionSecond, kii: Option<Kappa>, cubic: Option<FrictionCubic>) -> ViscosityHigher {
        let (t_r, c1, c2, ki) = (190.564, 1.0, 1.0, [1e-6, 2e-6, 3e-6]);
        let (ka, kr, kaa) = (kappa(1), kappa(1), kappa(3));
        ViscosityHigher::FrictionTheory(Box::new(FrictionTheory { t_r, c1, c2, ki, ka, kr, kaa, second, kii, cubic }))
    }

    /// Every viscosity form: each dilute term, each initial-density term (and none), each higher-order term and
    /// friction-theory variant, and each whole model.
    fn viscosities() -> Vec<ViscosityModel> {
        let dilutes = [
            ViscosityDilute::CollisionIntegral { terms: series(5), molar_mass: 0.044, c: 0.0266958 },
            ViscosityDilute::KineticTheory,
            ViscosityDilute::PowersOfT(series(2)),
            ViscosityDilute::PowersOfTr { terms: series(3), t_r: 190.564 },
            ViscosityDilute::CollisionIntegralPowersOfTstar { terms: series(4), t_r: 373.1, c: 1.6e-6 },
        ];
        let initials = [
            None,
            Some(ViscosityInitial::RainwaterFriend(series(9))),
            Some(ViscosityInitial::Empirical {
                n: lists::<1>(4)[0].clone(),
                d: lists::<2>(4)[1].clone(),
                t: lists::<3>(4)[2].clone(),
                t_r: 553.6,
                rho_r: 3.2e3,
            }),
        ];
        let rr = FrictionSecond::Rr(kappa(1));
        let cubic = FrictionCubic { krrr: kappa(1), kaaa: kappa(1) };
        let highers =
            [bh(), friction(rr, None, None), friction(FrictionSecond::Drdr(kappa(3)), Some(kappa(3)), Some(cubic))];
        let mut models = Vec::new();
        for (i, dilute) in dilutes.into_iter().enumerate() {
            let initial = initials[i % initials.len()].clone();
            let higher = highers[i % highers.len()].clone();
            models.push(ViscosityModel::Staged(StagedViscosity { dilute, initial, higher }));
        }
        let hardcoded_stages =
            HardcodedDiluteViscosity::ALL.iter().copied().zip(HardcodedHigherViscosity::ALL.iter().copied());
        for (dilute, higher) in hardcoded_stages {
            let (dilute, higher) = (ViscosityDilute::Hardcoded(dilute), ViscosityHigher::Hardcoded(higher));
            models.push(ViscosityModel::Staged(StagedViscosity { dilute, initial: None, higher }));
        }
        for &higher in HardcodedHigherViscosity::ALL {
            let higher = ViscosityHigher::Hardcoded(higher);
            models.push(ViscosityModel::Staged(StagedViscosity {
                dilute: ViscosityDilute::KineticTheory,
                initial: None,
                higher,
            }));
        }
        models.push(ViscosityModel::Ecs(EcsViscosity { reference: "R134a".into(), psi: series(3), psi_rho_r: 4.9e3 }));
        let (c_liq, c_vap) = ([0.1, -0.2, 0.3, -0.4], [1.0, 2.0, 3.0, 4.0]);
        models.push(ViscosityModel::RhoSrCs(RhoSrCs {
            c: 0.87302,
            c_liq,
            c_vap,
            rhosr_critical: -54468.48,
            x_crossover: 2.0,
        }));
        let chung = Chung {
            rho_c: 3820.0,
            t_c: 511.72,
            molar_mass: 0.0701329,
            dipole_moment: 0.1,
            acentric: 0.1521,
            kappa: 0.0,
        };
        models.push(ViscosityModel::Chung(chung));
        models.extend(HardcodedViscosity::ALL.iter().map(|&h| ViscosityModel::Hardcoded(h)));
        models
    }

    /// Every conductivity form.
    fn conductivities() -> Vec<ConductivityModel> {
        let dilutes = [
            ConductivityDilute::RatioOfPolynomials { numerator: series(4), denominator: series(2), t_r: 305.322 },
            ConductivityDilute::Eta0AndPoly(series(3)),
            ConductivityDilute::Hardcoded(HardcodedDiluteConductivity::CarbonDioxideHuberJpcrd2016),
            ConductivityDilute::Hardcoded(HardcodedDiluteConductivity::Ethane),
        ];
        let [b, t, d] = lists(5);
        let [a, tt, dd, gamma, l] = lists(4);
        let residuals = [
            ConductivityResidual::Polynomial { b, t, d, t_r: 425.125, rho_r: 228.0 },
            ConductivityResidual::PolynomialAndExponential { a, t: tt, d: dd, gamma, l },
        ];
        let os = OlchowySengers { q_d: 1.0 / 7.6e-10, ..OlchowySengers::COOLPROP };
        let mut criticals = vec![
            ConductivityCritical::OlchowySengers(os),
            ConductivityCritical::OlchowySengers(OlchowySengers { t_ref: Some(265.262), ..os }),
        ];
        criticals.extend(HardcodedCriticalConductivity::ALL.iter().map(|&h| ConductivityCritical::Hardcoded(h)));
        let mut models = Vec::new();
        for (i, critical) in criticals.into_iter().enumerate() {
            let (dilute, residual) = (dilutes[i % dilutes.len()].clone(), residuals[i % residuals.len()].clone());
            let reducing = Reducing { t: 305.322, rho: 6870.0, p: 4.8722e6 };
            models.push(ConductivityModel::Staged(StagedConductivity { dilute, residual, critical, reducing }));
        }
        let f_int = series(2);
        let ecs = EcsConductivity {
            reference: "Propane".into(),
            psi: series(2),
            psi_rho_r: 5000.0,
            f_int,
            f_int_t_r: 1.0,
            q_d: 917069412.9838686,
            reducing: Reducing { t: 395.425, rho: 3330.0, p: 4.1e6 },
        };
        models.push(ConductivityModel::Ecs(ecs));
        models.extend(HardcodedConductivity::ALL.iter().map(|&h| ConductivityModel::Hardcoded(h)));
        models.push(ConductivityModel::Methane(friend_methane()));
        models
    }

    /// Methane's form with its snapshots: v8.0.0's critical point (superancillaries on), `rhoV` and reducing state.
    pub(crate) fn friend_methane() -> FriendMethane {
        let rho_v = Ancillary {
            exponential: true,
            tau_r: true,
            t_r: 190.564,
            reducing: 10139.128,
            n: vec![
                -3.258091004501641,
                24.278993697870174,
                -25.256402428812585,
                -0.17494067187770526,
                0.3375209723239743,
                -2.3099699792209805,
            ],
            t: vec![0.392, 0.606, 0.631, 1.024, 2.03, 4.077],
            t_min: 90.6941,
            t_max: 190.564,
        };
        let reducing = Reducing { t: 190.564, rho: 10139.128, p: 4.5992e6 };
        FriendMethane { t_c: 190.56400265128698, rho_c: 10139.137654850272, rho_v, reducing }
    }

    /// Records covering every form, with and without Lennard-Jones parameters and DOIs, and a record with one model
    /// only.
    pub(crate) fn every_form() -> Vec<TransportRecord> {
        let (viscosities, conductivities) = (viscosities(), conductivities());
        let n = viscosities.len().max(conductivities.len());
        let lj = LennardJones { sigma: 0.4363e-9, epsilon_over_k: 192.0 };
        let mut records: Vec<TransportRecord> = (0..n)
            .map(|i| TransportRecord {
                viscosity: Some(Viscosity {
                    source: source("Vogel-JPCRD-1998", (i % 2 == 0).then_some("10.1063/1.556037")),
                    lennard_jones: (i % 3 != 0).then_some(lj),
                    model: viscosities[i % viscosities.len()].clone(),
                }),
                conductivity: Some(Conductivity {
                    source: source("Huber-RP912", None),
                    model: conductivities[i % conductivities.len()].clone(),
                }),
            })
            .collect();
        records.push(TransportRecord { viscosity: records[0].viscosity.clone(), conductivity: None });
        let ecs = |r: &&TransportRecord| matches!(r.viscosity.as_ref().map(|v| &v.model), Some(ViscosityModel::Ecs(_)));
        let both_ecs =
            |r: &&TransportRecord| matches!(r.conductivity.as_ref().map(|c| &c.model), Some(ConductivityModel::Ecs(_)));
        let viscosity = records.iter().find(ecs).and_then(|r| r.viscosity.clone());
        let conductivity = records.iter().find(both_ecs).and_then(|r| r.conductivity.clone());
        records.push(TransportRecord { viscosity, conductivity });
        records
    }

    fn with_transport(transport: TransportRecord) -> FluidRecord {
        let mut r = FluidRecord::synthetic("Toy").unwrap();
        r.transport = Some(transport);
        r
    }

    fn format_error(bytes: &[u8]) -> String {
        match FluidRecord::decode(bytes) {
            Err(LoadError::Format(m)) => m.into(),
            other => panic!("{other:?}"),
        }
    }

    /// PLAN.md M8.1: every form CoolProp v8.0.0 reaches round-trips through the blob's transport section bit for bit,
    /// and a fluid without models writes an empty section; the registry leaves the section in the blob until the first
    /// transport call (`decode_eager_parts`).
    #[test]
    fn every_form_round_trips_through_the_blob() {
        let records = every_form();
        assert_eq!(records.len(), 28);
        for transport in records {
            let record = with_transport(transport);
            assert_eq!(FluidRecord::decode(&record.encode()).as_ref(), Ok(&record));
            let eager = crate::blob::decode_eager_parts(&record.encode()).unwrap();
            assert_eq!(eager.transport, None);
            assert_eq!(crate::blob::decode_transport(&record.encode()).ok(), record.transport);
        }
        let none = with_transport(TransportRecord::default());
        assert!(crate::blob::sections(&none)[6].is_empty());
        assert_eq!(FluidRecord::decode(&none.encode()), Ok(none));
    }

    /// Map 05 R11: the decoder checks what datagen checked, so a pack that bypassed datagen cannot hand the evaluator
    /// unequal parallel lists or a short fixed array; a truncated section, an unknown tag, a bad presence byte and
    /// bytes left over are format errors too.
    #[test]
    fn transport_lengths_are_checked_on_decode() {
        let mut unequal = every_form().swap_remove(0);
        let Some(ViscosityModel::Staged(staged)) = unequal.viscosity.as_mut().map(|v| &mut v.model) else {
            panic!("the first record's viscosity is staged")
        };
        let ViscosityDilute::CollisionIntegral { terms, .. } = &mut staged.dilute else { panic!("collision integral") };
        terms.t = terms.t[1..].into();
        let err = format_error(&with_transport(unequal).encode());
        assert_eq!(err, "transport section: viscosity: model: dilute: t has 4 entries, a has 5");

        let mut short = every_form().swap_remove(0);
        let rhosr = viscosities().into_iter().find(|m| matches!(m, ViscosityModel::RhoSrCs(_))).unwrap();
        short.viscosity.as_mut().unwrap().model = rhosr;
        let blob_with = |edit: &dyn Fn(&mut Vec<u8>)| {
            let mut bodies = crate::blob::sections(&with_transport(short.clone()));
            edit(&mut bodies[6]);
            crate::blob::assemble(&bodies)
        };
        // The section: viscosity's presence byte, the bibkey (u32 length, bytes), the DOI's presence byte, the DOI,
        // the Lennard-Jones presence byte (absent in record 0), then the model's kind and ρ*_sr-CS's C and c_liq.
        let doi_flag = 1 + 4 + "Vogel-JPCRD-1998".len();
        let kind = doi_flag + 1 + 4 + "10.1063/1.556037".len() + 1;
        let c_liq_count = kind + 1 + 8;
        assert_eq!(blob_with(&|_| {}), with_transport(short.clone()).encode());
        type Edit<'a> = &'a dyn Fn(&mut Vec<u8>);
        let cases: [(Edit<'_>, &str); 8] = [
            (&|b| b[doi_flag] = 2, "transport section: viscosity: source: doi: presence byte 2"),
            (&|b| b[0] = 7, "transport section: viscosity: presence byte 7"),
            (&|b| b[kind] = 9, "transport section: viscosity: model: kind: unknown tag 9"),
            (&|b| b[kind] = 5, "transport section: viscosity: model: kind: unknown tag 5"),
            (&|b| b[c_liq_count] = 3, "transport section: viscosity: model: c_liq has 3 entries, not 4"),
            (
                &|b| b[c_liq_count] = 200,
                "transport section: viscosity: model: c_liq lists 200 numbers past the section's end",
            ),
            (&|b| b.push(0), "transport section has bytes left over (1)"),
            (&|b| _ = b.pop(), "transport section: conductivity: model: reducing: transport section is truncated"),
        ];
        for (edit, want) in cases {
            assert_eq!(format_error(&blob_with(edit)), want);
        }
    }

    /// E15: the transport edits replace the value they name in the model that has it, and refuse a model without
    /// it; an edit of another part of the record leaves the transport record alone.
    #[test]
    fn transport_edits_apply_to_their_model() {
        let ecs = every_form()
            .into_iter()
            .find(|r| matches!(r.conductivity.as_ref().map(|c| &c.model), Some(ConductivityModel::Ecs(_))))
            .unwrap();
        let mut edited = ecs.clone();
        edited.apply(Edit::EcsQd(2e9)).unwrap();
        let Some(ConductivityModel::Ecs(e)) = edited.conductivity.as_ref().map(|c| &c.model) else { panic!() };
        assert_eq!(e.q_d, 2e9);
        let err = edited.apply(Edit::RhosrCritical(-54095.5));
        assert_eq!(err, Err(LoadError::Format("no rhosr-CS viscosity to edit".into())));

        let mut rhosr = ecs.clone();
        rhosr.viscosity.as_mut().unwrap().model =
            viscosities().into_iter().find(|m| matches!(m, ViscosityModel::RhoSrCs(_))).unwrap();
        rhosr.conductivity = None;
        rhosr.apply(Edit::RhosrCritical(-54095.5)).unwrap();
        let Some(ViscosityModel::RhoSrCs(r)) = rhosr.viscosity.as_ref().map(|v| &v.model) else { panic!() };
        assert_eq!(r.rhosr_critical, -54095.5);
        assert_eq!(rhosr.apply(Edit::EcsQd(1.0)), Err(LoadError::Format("no ECS conductivity to edit".into())));

        let mut untouched = ecs.clone();
        untouched.apply(Edit::MolarMass(0.1)).unwrap();
        assert_eq!(untouched, ecs);
    }

    /// Under `Corrected` a record applies its transport edits when its transport is decoded, and leaves them to the
    /// registry's transport closure when it is not (`decode_eager_parts`); `Parity` applies none.
    #[test]
    fn a_record_applies_its_transport_edits_once_decoded() {
        let ecs = every_form()
            .into_iter()
            .find(|r| matches!(r.conductivity.as_ref().map(|c| &c.model), Some(ConductivityModel::Ecs(_))))
            .unwrap();
        let mut record = with_transport(ecs);
        record.corrections = vec![Patch { divergence: "DIV-0099".into(), edit: Edit::EcsQd(2e9) }];
        let q_d = |r: &FluidRecord| match r.transport.as_ref().and_then(|t| t.conductivity.as_ref()).map(|c| &c.model) {
            Some(ConductivityModel::Ecs(e)) => e.q_d,
            _ => f64::NAN,
        };
        let mut parity = record.clone();
        parity.apply(crate::DataSet::Parity).unwrap();
        assert_eq!(q_d(&parity), 917069412.9838686);
        let mut corrected = record.clone();
        corrected.apply(crate::DataSet::Corrected).unwrap();
        assert_eq!((q_d(&corrected), &corrected.applied[..]), (2e9, &[Box::<str>::from("DIV-0099")][..]));
        let mut eager = crate::blob::decode_eager_parts(&record.encode()).unwrap();
        eager.apply(crate::DataSet::Corrected).unwrap();
        assert_eq!(eager.transport, None);
    }

    /// The registry's transport closure applies the transport edits under `Corrected` and none under `Parity`.
    #[test]
    fn the_registry_applies_transport_edits_under_corrected_only() {
        let ecs = every_form().pop().unwrap();
        let patches = [
            Patch { divergence: "DIV-0003".into(), edit: Edit::ReducingDensity(1.0) },
            Patch { divergence: "DIV-0099".into(), edit: Edit::EcsQd(2e9) },
        ];
        let q_d = |r: &TransportRecord| match r.conductivity.as_ref().map(|c| &c.model) {
            Some(ConductivityModel::Ecs(e)) => e.q_d,
            _ => f64::NAN,
        };
        let parity = ecs.clone().corrected(&patches, DataSet::Parity).unwrap();
        assert_eq!(parity, ecs);
        assert_eq!(q_d(&ecs.clone().corrected(&patches, DataSet::Corrected).unwrap()), 2e9);
        let rhosr = [Patch { divergence: "DIV-0099".into(), edit: Edit::RhosrCritical(1.0) }];
        let err = ecs.corrected(&rhosr, DataSet::Corrected);
        assert_eq!(err, Err(LoadError::Format("no rhosr-CS viscosity to edit".into())));
    }

    /// A record compiled without the registry carries its transport models: a fluid with one evaluates it or says when
    /// its evaluation lands, a fluid without one answers `NoModel`.
    #[test]
    fn a_compiled_record_carries_its_transport() {
        use crate::{Density, FlashOptions, Input, Phase, Temperature};
        let answers = |transport: TransportRecord| {
            let fluid = Fluid::new(std::sync::Arc::new(with_transport(transport).compile().unwrap()));
            let input = Input::dt(Density::molar(100.0).unwrap(), Temperature::new(300.0).unwrap());
            let state = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
            [Prop::Viscosity, Prop::Conductivity].map(|prop| fluid.prop(&state, prop))
        };
        let lands = |what: &str| Err(Error::Load(LoadError::Format(what.into())));
        let [viscosity, conductivity] = answers(every_form().swap_remove(1));
        assert!(viscosity.as_ref().is_ok_and(|eta| eta.is_finite()), "{viscosity:?}");
        assert!(conductivity.as_ref().is_ok_and(|lambda| lambda.is_finite()), "{conductivity:?}");
        // The critical term reads the fluid's η: where η lands later, so does λ, enhanced at this state or not.
        let missing_lj = lands("the Chung estimate of missing Lennard-Jones parameters lands at M8.9");
        assert_eq!(answers(every_form().swap_remove(0)), [missing_lj.clone(), missing_lj]);
        let none = [Err(Error::NoModel { prop: Prop::Viscosity }), Err(Error::NoModel { prop: Prop::Conductivity })];
        assert_eq!(answers(TransportRecord::default()), none);
    }

    /// The terms no v8.0.0 data exercises, against their closed forms at a Toy state: the empirical initial-density term
    /// (CycloHexane's only; its reducing constants are 1, and its hard-coded dilute term lands at M8.9), and the
    /// Batschinski-Hildebrand free-volume sum with τ in it (every v8.0.0 t₂ is 0).
    #[test]
    fn unexercised_viscosity_terms_match_their_closed_forms() {
        use crate::{Density, FlashOptions, Input, Phase, Temperature};
        let fluid = Fluid::new(std::sync::Arc::new(FluidRecord::synthetic("Toy").unwrap().compile().unwrap()));
        let (t, rho) = (300.0, 100.0);
        let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
        let state = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
        let eos = FluidRecord::synthetic("Toy").unwrap().eos.compile().unwrap();
        let x = TransportInputs::new(&state, 0.1, &eos);
        let one = |v: f64| -> Box<[f64]> { Box::new([v]) };
        let bh = BatschinskiHildebrand {
            t_r: 350.0,
            rho_r: 4000.0,
            a: one(1e-6),
            t1: one(1.0),
            d1: one(2.0),
            gamma: one(0.0),
            l: one(1.0),
            f: one(2e-7),
            t2: one(0.5),
            d2: one(1.0),
            g: one(3.0),
            h: one(0.0),
            p: one(1.0),
            q: one(0.0),
        };
        let model = StagedViscosity {
            dilute: ViscosityDilute::PowersOfT(Series::new(vec![1e-5], vec![0.0]).unwrap()),
            initial: Some(ViscosityInitial::Empirical {
                n: Box::new([2e-7, -3e-6, 4e-5]),
                d: Box::new([1.0, 2.0, 1.0]),
                t: Box::new([0.0, 1.0, 2.0]),
                t_r: 350.0,
                rho_r: 4000.0,
            }),
            higher: ViscosityHigher::BatschinskiHildebrand(Box::new(bh)),
        };
        let stages = viscosity::stages(&model, None, &x).unwrap();
        let (tau, delta) = (350.0 / t, rho / 4000.0);
        let empirical = 2e-7 * delta - 3e-6 * delta * delta * tau + 4e-5 * delta * tau * tau;
        let free_volume =
            1e-6 * delta * delta * tau + 2e-7 * delta * math::sqrt(tau) * (1.0 / (3.0 - delta) - 1.0 / 3.0);
        let close = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-14;
        assert!(close(stages.initial_density, empirical), "{} against {empirical}", stages.initial_density);
        assert!(close(stages.residual, free_volume), "{} against {free_volume}", stages.residual);
        assert_eq!(stages.dilute, 1e-5);
    }

    /// Map 05 R6: the Olchowy-Sengers term reads the η it is given, never a viscosity it finds: Nitrogen near its
    /// critical point with η doubled has half the enhancement, and the other stages do not move.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn the_critical_term_reads_the_viscosity_it_is_given() {
        use crate::{Density, Input, Temperature};
        let registry = crate::Registry::embedded().unwrap();
        let record = registry.record("Nitrogen").unwrap();
        let fluid = registry.get("Nitrogen").unwrap();
        let state =
            fluid.state(Input::dt(Density::molar(11_000.0).unwrap(), Temperature::new(130.0).unwrap())).unwrap();
        let eos = record.eos.compile().unwrap();
        let x = TransportInputs::new(&state, record.molar_mass, &eos);
        let transport = record.transport.unwrap();
        let (viscosity, conductivity) = (transport.viscosity.unwrap(), transport.conductivity.unwrap());
        let eta = viscosity.stages(&x, &[]).unwrap();
        let doubled = ViscosityStages {
            dilute: 2.0 * eta.dilute,
            initial_density: 2.0 * eta.initial_density,
            residual: 2.0 * eta.residual,
            critical: 2.0 * eta.critical,
        };
        let (once, twice) = (
            conductivity.stages(&x, &Ok(eta), None, &[]).unwrap(),
            conductivity.stages(&x, &Ok(doubled), None, &[]).unwrap(),
        );
        assert!(once.critical > 1e-3 * once.total(), "{once:?}");
        assert!((twice.critical / once.critical - 0.5).abs() < 1e-15, "{once:?} {twice:?}");
        // eta0_and_poly reads η₀ too: Nitrogen's dilute term moves with it, its residual term does not.
        assert!(twice.dilute != once.dilute && twice.residual == once.residual);
    }

    /// Rot: ROT-117 (map 05 R10). Methane's conductivity (Friend, Ely and Ingham 1989) reads a critical point and a
    /// saturated-vapour density. CoolProp reads them through a configuration flag and an ancillary (1.06e-7 apart at
    /// 185 K and 80 kg/m³ across the flag); here they are the record's, snapshotted at datagen, so the same fluid built
    /// without its superancillary, whose critical point and saturation then come from its VLE, answers bit for bit the
    /// same: in the vapour below T_c, where the ancillary is read, in the liquid and above T_c.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn methane_lambda_is_independent_of_the_saturation_source() {
        use crate::{Density, FlashOptions, Input, Phase, Temperature};
        let record = crate::Registry::embedded().unwrap().record("Methane").unwrap();
        let model = record.transport.as_ref().and_then(|t| t.conductivity.as_ref()).map(|c| &c.model);
        let Some(ConductivityModel::Methane(m)) = model else {
            panic!("Methane's conductivity is Friend's: {model:?}")
        };
        assert_eq!((m.t_c, m.rho_c), (190.56400265128698, 10139.137654850272));
        assert!(185.0 < m.t_c && 80.0 / record.molar_mass < m.rho_c, "185 K and 80 kg/m³ read the ancillary");
        let mut bare = record.clone();
        (bare.superancillary, bare.superancillary_fit) = (None, None);
        let fluids = [record, bare].map(|r| Fluid::new(std::sync::Arc::new(r.compile().unwrap())));
        let lambda = |fluid: &Fluid, t: f64, rho: f64, phase: Phase| {
            let input = Input::dt(Density::mass(rho).unwrap(), Temperature::new(t).unwrap());
            let state = fluid.flash(input, &FlashOptions::new().with_phase(phase)).unwrap();
            fluid.prop(&state, Prop::Conductivity).unwrap()
        };
        let states = [
            (185.0, 80.0, Phase::Gas),
            (150.0, 10.0, Phase::Gas),
            (120.0, 380.0, Phase::Liquid),
            (300.0, 50.0, Phase::Supercritical),
        ];
        for (t, rho, phase) in states {
            let [with, without] = [&fluids[0], &fluids[1]].map(|f| lambda(f, t, rho, phase));
            assert_eq!(with.to_bits(), without.to_bits(), "{t} K, {rho} kg/m³");
            assert!(with.is_finite() && with > 0.0, "{t} K, {rho} kg/m³: {with}");
        }
    }

    /// A record compiled without the registry carries its surface-tension fit, with transport models or without.
    #[test]
    fn a_compiled_record_carries_its_surface_tension() {
        let terms = Series::new(vec![0.05], vec![1.25]).unwrap();
        let fit = SurfaceTension { source: source("Mulero-JPCRD-2012", None), t_c: 300.0, terms };
        let sigma = |transport: Option<TransportRecord>| {
            let mut record = FluidRecord::synthetic("Toy").unwrap();
            (record.transport, record.surface_tension) = (transport, Some(fit.clone()));
            Fluid::new(std::sync::Arc::new(record.compile().unwrap())).model().surface_tension(200.0)
        };
        let want = 0.05 * math::powf(1.0 - 200.0 / 300.0, 1.25);
        assert_eq!(sigma(None), Ok(want));
        assert_eq!(sigma(every_form().pop()), Ok(want));
    }

    /// The ECS reference fluids a record names, viscosity's first; none for the other forms.
    #[test]
    fn references_are_the_ecs_models_reference_fluids() {
        let records = every_form();
        let named: Vec<Vec<&str>> = records.iter().map(TransportRecord::references).filter(|r| !r.is_empty()).collect();
        assert!(named.iter().any(|r| r[..] == ["R134a", "Propane"]), "{named:?}");
        assert!(named.iter().any(|r| r[..] == ["R134a"]), "{named:?}");
        assert!(named.iter().any(|r| r[..] == ["Propane"]), "{named:?}");
        assert!(TransportRecord::default().references().is_empty());
    }

    /// The hardcoded models are found by CoolProp's names, and only in their own slot; the dead ones are not there
    /// (ROT-116).
    #[test]
    fn hardcoded_models_are_found_by_coolprops_names() {
        assert_eq!(HardcodedViscosity::from_name("m-Xylene"), Some(HardcodedViscosity::MXylene));
        assert_eq!(HardcodedViscosity::MXylene.name(), "m-Xylene");
        assert_eq!(HardcodedCriticalConductivity::from_name("None"), Some(HardcodedCriticalConductivity::Absent));
        assert_eq!(HardcodedDiluteConductivity::from_name("CO2"), None, "Vesovic's dead term");
        assert_eq!(HardcodedCriticalConductivity::from_name("CarbonDioxideScalabrinJPCRD2006"), None);
        assert_eq!(HardcodedConductivity::from_name("Methanol"), None, "Methanol's conductivity is staged");
    }

    /// The embedded fluids with a surface-tension fit: (name, fit, fluid).
    #[cfg(feature = "fluids-all")]
    fn fits() -> Vec<(&'static str, SurfaceTension, Fluid)> {
        let registry = crate::Registry::embedded().unwrap();
        let fits = phasekit_data::FLUIDS.iter().filter_map(|f| {
            let fit = registry.record(f.name).unwrap().surface_tension?;
            Some((f.name, fit, registry.get(f.name).unwrap().clone()))
        });
        fits.collect()
    }

    /// Rot: ROT-114 (map 05 R13). σ is a pure function of T over its fit's range, from the fluid's T_min to the lower of
    /// the fit's own T_c and the model's critical temperature. Between the two, where CoolProp throws ("T > Tc"), it is a
    /// `Domain` error, never 0 and never an extrapolation (user decision 6). One case per affected fluid: 62 of the 108
    /// fits have T_c,σ below the model's T_c, map 05's nine (measured against the published critical point) among them,
    /// and 20 by more than 1 mK.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn surface_tension_between_tc_sigma_and_tc_eos() {
        let fits = fits();
        assert_eq!(fits.len(), 108);
        let mut affected = Vec::new();
        for (name, fit, fluid) in &fits {
            let t_c = fluid.model().critical_point().unwrap().t;
            let model = fluid.model();
            if fit.t_c < t_c {
                let t = 0.5 * (fit.t_c + t_c);
                let refused = DomainError::AboveMaxTemperature { t, t_max: fit.t_c };
                assert_eq!(model.surface_tension(t), Err(refused.into()), "{name}");
                assert_eq!(model.surface_tension(fit.t_c), Ok(0.0), "{name}: σ(T_c,σ) = 0");
                affected.push((*name, t_c - fit.t_c > 1e-3));
            } else {
                let refused = DomainError::AboveMaxTemperature { t: fit.t_c, t_max: t_c };
                assert!(fit.t_c == t_c || model.surface_tension(fit.t_c) == Err(refused.into()), "{name}");
            }
        }
        let names: Vec<&str> = affected.iter().map(|(name, _)| *name).collect();
        let map05 = ["Ammonia", "CarbonDioxide", "Ethanol", "IsoButane", "R1234ze(E)", "R404A", "R407C", "R507A"];
        assert!(map05.iter().chain(["SulfurHexafluoride"].iter()).all(|f| names.contains(f)), "{names:?}");
        assert_eq!((affected.len(), affected.iter().filter(|(_, wide)| *wide).count()), (62, 20));
        let (name, _, fluid) = &fits[0];
        let t_min = crate::Registry::embedded().unwrap().record(name).unwrap().limits.t_min();
        let below = DomainError::BelowMinTemperature { t: t_min - 1.0, t_min };
        assert_eq!(fluid.model().surface_tension(t_min - 1.0), Err(below.into()), "{name}");
        let nan = fluid.model().surface_tension(f64::NAN);
        assert!(matches!(nan, Err(Error::InvalidInput { quantity: "T", .. })), "{nan:?}");
    }

    /// Rot: ROT-123 (map 10 §8.4). Water's σ is CoolProp's model, Mulero 2012's two-term fit, and its source says so,
    /// not IAPWS. Against IAPWS R1-76(2014)'s equation (page 3: σ = Bτ^µ(1 + bτ), τ = 1 − T/T_c, T_c = 647.096 K,
    /// B = 235.8 mN/m, b = −0.625, µ = 1.256) it is 0.12 % high at 300 K and 0.90 % low at 600 K: IAPWS σ is a separate,
    /// labelled comparison, not this model's arbiter.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn water_sigma_is_labelled_coolprop_model() {
        let (_, fit, water) = fits().into_iter().find(|(name, ..)| *name == "Water").unwrap();
        assert_eq!((&*fit.source.bibkey, fit.t_c, fit.terms.a.len()), ("Mulero-JPCRD-2012", 647.096, 2));
        let iapws = |t: f64| {
            let tau = 1.0 - t / 647.096;
            235.8e-3 * math::powf(tau, 1.256) * (1.0 - 0.625 * tau)
        };
        let percent = |t: f64| format!("{:+.2}", 100.0 * (water.model().surface_tension(t).unwrap() / iapws(t) - 1.0));
        assert_eq!((percent(300.0), percent(600.0)), ("+0.12".to_string(), "-0.90".to_string()));
    }
}
