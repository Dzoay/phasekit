//! L3 data: where bytes come from ([`DataSource`], [`Blob`], [`Pack`]), the decoded mutable record
//! ([`FluidRecord`]: the arbitration seam, semver-exempt via `phasekit_core::internal`), the corrections overlay
//! ([`Patch`]) and the two datasets ([`DataSet`]).
//! Pipeline: blob → `FluidRecord::decode` → `apply(DataSet)` → `compile` → `PureFluid` (D7).

use core::fmt;
use std::sync::Arc;

use crate::error::{Error, LoadError};
use crate::fluid::{PureFluid, PureFluidBuilder};
use crate::helmholtz::{
    DoubleExponentialTerm, GaoBTerm, GaussianTerm, IdealGas, IdealTerm, Lemmon2005Term, MultiParameterEos,
    NonAnalyticTerm, OffsetReference, PowerBlock, PowerTerm, ResidualBlock,
};
use crate::model::{CriticalPoint, DataTerms, FluidInfo, Limits, ModelKey, Source};

/// Position of a data-backed fluid in its source's index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FluidId(pub u32);

/// Encoded bytes of one fluid: embedded (zero-copy) or owned (fetched in a browser, read from a file).
#[derive(Clone, Debug)]
pub enum Blob {
    /// `include_bytes!` data.
    Static(&'static [u8]),
    /// Runtime bytes, shared without copying.
    Shared(Arc<[u8]>),
}

impl Blob {
    /// The bytes.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Blob::Static(b) => b,
            Blob::Shared(b) => b,
        }
    }
}

/// Where a registry layer gets bytes. `names` and `references` are read once, when the layer is built;
/// `blob` is the cold path, called at most once per fluid per layer. The core does no filesystem or network
/// I/O: a native app or a browser supplies a source (map 11 U8, U15; Kernel graft).
pub trait DataSource: Send + Sync + fmt::Debug {
    /// Every fluid's names, canonical first, then aliases and CAS; the outer position is its [`FluidId`].
    fn names(&self) -> Vec<Vec<String>>;
    /// The blob of one fluid.
    fn blob(&self, id: FluidId) -> Result<Blob, LoadError>;
    /// Canonical names of the fluids `id`'s lazy parts need (ECS reference fluids). The registry resolves
    /// them, and refuses missing or cyclic references, when the layer is built (E5, map 05 R7).
    fn references(&self, _id: FluidId) -> Vec<String> {
        Vec::new()
    }
}

/// A runtime pack: the blobs of several fluids plus their index in one buffer (a browser fetch, a file).
/// JS reaches it as `registry.withPack(bytes)` (E6).
#[derive(Debug)]
pub struct Pack {
    index: Vec<Vec<String>>,
    requires: Vec<Vec<String>>,
    blobs: Vec<Arc<[u8]>>,
}

impl Pack {
    /// Parses and validates a pack (header, checksum, index, section table).
    pub fn new(bytes: Arc<[u8]>) -> Result<Pack, LoadError> {
        Err(LoadError::Format(format!("pack format lands at M2 ({} bytes)", bytes.len()).into()))
    }
}

impl DataSource for Pack {
    fn names(&self) -> Vec<Vec<String>> {
        self.index.clone()
    }
    fn blob(&self, id: FluidId) -> Result<Blob, LoadError> {
        let bytes = self.blobs.get(id.0 as usize).ok_or(LoadError::Format("fluid id out of range".into()))?;
        Ok(Blob::Shared(Arc::clone(bytes)))
    }
    fn references(&self, id: FluidId) -> Vec<String> {
        self.requires.get(id.0 as usize).cloned().unwrap_or_default()
    }
}

/// Which dataset a registry layer compiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DataSet {
    /// v8.0.0 data plus the cited corrections overlay: the default.
    #[default]
    Corrected,
    /// Bit-exact v8.0.0 data: oracle fixtures compare against this (Verification-first).
    Parity,
}

/// One typed edit of the overlay. Closed: datagen validates every row of `data/corrections.csv` against it
/// and checks the v8.0.0 value it replaces. Transport-coefficient edits join with the M8 transport section.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Edit {
    /// Replace the EOS gas constant (R1234ze(E): the paper's R, map 13 §3). Rescales the SA exactly.
    GasConstant(f64),
    /// Replace the reducing density, mol/m³ (Nitrogen 11183.9, map 12 §6.3). Rescales the SA exactly.
    ReducingDensity(f64),
    /// Replace the molar mass, kg/mol (Ethylene, OrthoHydrogen, map 12 §6.3). The SA is molar: untouched.
    MolarMass(f64),
    /// Replace p0 of one melting-curve segment, Pa (Water ice VI 632.4 MPa, map 10).
    MeltingP0 {
        /// Segment index.
        segment: u8,
        /// New reference pressure.
        p0: f64,
    },
}

/// A correction shipped with the data it patches, citing the divergence-register entry that proves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    /// Divergence-register id, e.g. `"DIV-0003"` (cross-checked by `phasekit-verify/tests/register.rs`).
    pub divergence: Box<str>,
    /// The edit.
    pub edit: Edit,
}

/// EOS constants and terms as decoded: one list per residual kind (map 02 §3.1), the α⁰ terms in file order.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[allow(missing_docs)] // the symbols of the EOS
pub struct EosRecord {
    pub gas_constant: f64,
    pub t_reducing: f64,
    pub rho_reducing: f64,
    /// Upper molar-density bound for root bracketing. Datagen's bound is provisional: the EOS's saturated-liquid
    /// density at T_min (`STATES.sat_min_liquid`), until M7.1 can solve ρ(T_min, p_max) *(inference)*.
    pub rho_max: f64,
    /// CoolProp's Power and Exponential kinds.
    pub power: Vec<PowerTerm>,
    pub lemmon2005: Vec<Lemmon2005Term>,
    pub double_exponential: Vec<DoubleExponentialTerm>,
    pub gaussian: Vec<GaussianTerm>,
    pub gao_b: Vec<GaoBTerm>,
    pub non_analytic: Vec<NonAnalyticTerm>,
    pub ideal: Vec<IdealTerm>,
}

impl EosRecord {
    /// An EOS with its constants and no terms yet.
    pub fn new(gas_constant: f64, t_reducing: f64, rho_reducing: f64, rho_max: f64) -> EosRecord {
        EosRecord {
            gas_constant,
            t_reducing,
            rho_reducing,
            rho_max,
            power: Vec::new(),
            lemmon2005: Vec::new(),
            double_exponential: Vec::new(),
            gaussian: Vec::new(),
            gao_b: Vec::new(),
            non_analytic: Vec::new(),
            ideal: Vec::new(),
        }
    }

    /// The canonical bytes of the EOS section: the ONE encoder, shared by datagen (which writes it into the
    /// blob) and the runtime hash gate (E14). Scale constants first, then the shape.
    pub fn encode(&self, out: &mut Vec<u8>) {
        for x in [self.gas_constant, self.rho_reducing, self.rho_max] {
            out.extend_from_slice(&x.to_le_bytes());
        }
        self.encode_shape(out);
    }

    /// Everything in α except R and ρ_r (and the bracketing bound ρ_max): the part a superancillary
    /// depends on beyond an exact rescaling.
    pub fn encode_shape(&self, out: &mut Vec<u8>) {
        let mut put = |tag: u8, xs: &[f64]| {
            out.push(tag);
            xs.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        };
        put(0, &[self.t_reducing]);
        for p in &self.power {
            put(1, &[p.n, p.t, f64::from(p.d), f64::from(p.l), p.c]);
        }
        for p in &self.lemmon2005 {
            put(2, &[p.n, p.t, f64::from(p.d), f64::from(p.l), p.m]);
        }
        for p in &self.double_exponential {
            put(3, &[p.n, p.t, f64::from(p.d), p.gd, f64::from(p.ld), p.gt, p.lt]);
        }
        for p in &self.gaussian {
            put(4, &[p.n, p.t, f64::from(p.d), p.eta, p.epsilon, p.beta, p.gamma]);
        }
        for p in &self.gao_b {
            put(5, &[p.n, p.t, f64::from(p.d), p.eta, p.epsilon, p.beta, p.gamma, p.b]);
        }
        for p in &self.non_analytic {
            put(6, &[p.n, p.a, p.b, p.beta, p.big_a, p.big_b, p.big_c, p.big_d]);
        }
        for term in &self.ideal {
            match *term {
                IdealTerm::Lead { a1, a2 } => put(10, &[a1, a2]),
                IdealTerm::LogTau { a } => put(11, &[a]),
                IdealTerm::Power { n, t } => put(12, &[n, t]),
                IdealTerm::PlanckEinstein { n, theta } => put(13, &[n, theta]),
                IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => put(14, &[n, theta, c, d]),
                IdealTerm::Cp0Power { c, t, tc, t0 } => put(15, &[c, t, tc, t0]),
                IdealTerm::Offset { a1, a2, reference } => {
                    let tag = match reference {
                        OffsetReference::Iir => 0.0,
                        OffsetReference::Nbp => 1.0,
                        OffsetReference::Other => 2.0,
                        OffsetReference::Custom => 3.0,
                    };
                    put(16, &[a1, a2, tag]);
                }
            }
        }
    }

    /// Hash of the whole canonical EOS section.
    pub fn eos_hash(&self) -> ModelKey {
        let mut bytes = Vec::new();
        self.encode(&mut bytes);
        ModelKey::from_content(&bytes)
    }

    /// Hash of the shape only.
    pub fn shape_hash(&self) -> ModelKey {
        let mut bytes = Vec::new();
        self.encode_shape(&mut bytes);
        ModelKey::from_content(&bytes)
    }
}

/// One melting-curve segment (Simon-type form, map 07); the full form lands at M8.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct MeltingSegment {
    pub t0: f64,
    pub p0: f64,
    pub t_min: f64,
    pub t_max: f64,
}

/// What the shipped superancillary was fitted to. Datagen verifies CoolProp's `source_eos_hash` stamp
/// (map 09: all 130 recompute) and records this one in the blob.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct SaStamp {
    pub shape: ModelKey,
    pub gas_constant: f64,
    pub rho_reducing: f64,
}

/// The hash gate (D7, E14): how the corrected EOS relates to the superancillary shipped with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SaFreshness {
    /// Fitted to exactly this EOS: `SatAccuracy::Exact`.
    Fresh,
    /// Only R and/or ρ_r changed. Saturation is invariant in (τ, δ), so ρ′, ρ″ scale by `rho` and p_sat by
    /// `p` exactly (p = ρRT(1 + δα^r_δ)); still `Exact` (inference, proved at M6 against VLE).
    Rescaled {
        /// Factor on p_sat: (R′/R)·(ρ_r′/ρ_r).
        p: f64,
        /// Factor on ρ′ and ρ″: ρ_r′/ρ_r.
        rho: f64,
    },
    /// Any other EOS change: `SatAccuracy::Guess`, every answer polished by VLE until datagen refits.
    Stale,
}

/// Environmental and safety metadata (CoolProp's `INFO.ENVIRONMENTAL`, map 09 §4.2). CoolProp built it from a DTU
/// table that reads REFPROP files and states no source or licence (map 09 §9), so it is restricted data: carried in
/// the record with `source.terms = Restricted` and never written into the default blobs (D14). CoolProp's sentinels
/// (−1 and ±10^n for "not specified", "UNKNOWN" and "?") are `None` (ROT-057).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Environmental {
    /// ASHRAE 34 safety class (`A1`, `A2L`, `B2`, ...).
    pub ashrae34: Option<Box<str>>,
    /// Global warming potentials over 20, 100 and 500 years, relative to CO₂.
    pub gwp20: Option<f64>,
    #[allow(missing_docs)]
    pub gwp100: Option<f64>,
    #[allow(missing_docs)]
    pub gwp500: Option<f64>,
    /// Ozone depletion potential, relative to R11.
    pub odp: Option<f64>,
    /// NFPA 704 health, flammability and physical-hazard ratings, 0-4.
    pub health: Option<u8>,
    #[allow(missing_docs)]
    pub flammability: Option<u8>,
    #[allow(missing_docs)]
    pub physical: Option<u8>,
    /// Where the values come from, and their terms.
    pub source: Source,
}

impl Environmental {
    /// Metadata from `source` with every value absent.
    pub fn new(source: Source) -> Environmental {
        Environmental {
            ashrae34: None,
            gwp20: None,
            gwp100: None,
            gwp500: None,
            odp: None,
            health: None,
            flammability: None,
            physical: None,
            source,
        }
    }
}

/// A decoded fluid as plain, mutable data: the arbitration seam. Swap one constant for the paper's value,
/// `compile`, and re-run the paper's own check table (map 13 §3 protocol).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[allow(missing_docs)]
pub struct FluidRecord {
    pub name: String,
    /// The aliases CoolProp lists, verbatim (identifiers below are kept apart).
    pub aliases: Vec<String>,
    /// CAS number, or CoolProp's synthetic id for a blend (`AIR.PPF`; map 09 R16).
    pub cas: Option<String>,
    /// REFPROP's name for the fluid, when it has one (CoolProp writes "N/A" otherwise; ROT-057).
    pub refprop_name: Option<String>,
    pub inchi_key: Option<String>,
    pub molar_mass: f64,
    pub source: Source,
    pub eos: EosRecord,
    pub limits: Limits,
    pub critical: Option<CriticalPoint>,
    pub melting: Vec<MeltingSegment>,
    /// What the shipped superancillary was fitted to, if the fluid has one.
    pub superancillary_fit: Option<SaStamp>,
    /// Corrections shipped with this fluid; applied only under [`DataSet::Corrected`].
    pub corrections: Vec<Patch>,
    /// Divergence ids actually applied.
    pub applied: Vec<Box<str>>,
    /// Restricted metadata, never shipped by default (see [`Environmental`]).
    pub environmental: Option<Environmental>,
}

impl FluidRecord {
    /// A record with its identity, EOS and limits; everything else empty.
    pub fn new(name: &str, molar_mass: f64, source: Source, eos: EosRecord, limits: Limits) -> FluidRecord {
        FluidRecord {
            name: name.into(),
            aliases: Vec::new(),
            cas: None,
            refprop_name: None,
            inchi_key: None,
            molar_mass,
            source,
            eos,
            limits,
            critical: None,
            melting: Vec::new(),
            superancillary_fit: None,
            corrections: Vec::new(),
            applied: Vec::new(),
            environmental: None,
        }
    }

    /// Decodes and validates a versioned little-endian blob (M2: header, checksum, section table).
    /// Sketch stand-in until M2: `PKIT\0toy:<name>` decodes to a small fixed record named `<name>`, so the
    /// registry's lazy-load, layering and reference tests run end to end.
    pub fn decode(bytes: &[u8]) -> Result<FluidRecord, LoadError> {
        let [b'P', b'K', b'I', b'T', 0, rest @ ..] = bytes else {
            return Err(LoadError::Format("not a phasekit blob".into()));
        };
        let Some(name) = rest.strip_prefix(b"toy:") else {
            return Err(LoadError::Format("blob format v1 lands at M2".into()));
        };
        let name = core::str::from_utf8(name).map_err(|_| LoadError::Format("name is not UTF-8".into()))?;
        FluidRecord::toy(name).map_err(|e| LoadError::Format(e.to_string().into()))
    }

    /// The sketch's toy record: an R1234ze(E)-like two-term EOS (no superancillary, no corrections).
    pub fn toy(name: &str) -> Result<FluidRecord, Error> {
        let mut eos = EosRecord::new(8.314_472, 382.513, 4290.0, 20_000.0);
        eos.power = vec![PowerTerm::new(0.03, 1.0, 4, 0, 0.0), PowerTerm::new(0.5, 1.5, 2, 1, 1.0)];
        eos.ideal = vec![IdealTerm::Lead { a1: -12.5, a2: 8.6 }, IdealTerm::LogTau { a: 3.0 }];
        let limits = Limits::new(169.0, 420.0, 100e6)?;
        let source = Source { bibkey: "toy".into(), doi: None, terms: DataTerms::Published };
        Ok(FluidRecord::new(name, 0.114_041_6, source, eos, limits))
    }

    /// Applies the shipped corrections when `set` is `Corrected`; a no-op for `Parity`.
    pub fn apply(&mut self, set: DataSet) -> Result<(), LoadError> {
        if set == DataSet::Parity {
            return Ok(());
        }
        for patch in &self.corrections {
            match patch.edit {
                Edit::GasConstant(r) => self.eos.gas_constant = r,
                Edit::ReducingDensity(rho) => self.eos.rho_reducing = rho,
                Edit::MolarMass(m) => self.molar_mass = m,
                Edit::MeltingP0 { segment, p0 } => {
                    let seg = self.melting.get_mut(usize::from(segment));
                    seg.ok_or(LoadError::Format("melting segment out of range".into()))?.p0 = p0;
                }
            }
            self.applied.push(patch.divergence.clone());
        }
        Ok(())
    }

    /// The hash gate. `None` when the fluid ships no superancillary.
    pub fn superancillary_freshness(&self) -> Option<SaFreshness> {
        let fit = self.superancillary_fit?;
        Some(if self.eos.shape_hash() != fit.shape {
            SaFreshness::Stale
        } else {
            let rho = self.eos.rho_reducing / fit.rho_reducing;
            let p = rho * (self.eos.gas_constant / fit.gas_constant);
            if rho == 1.0 && p == 1.0 { SaFreshness::Fresh } else { SaFreshness::Rescaled { p, rho } }
        })
    }

    /// Content key of the compiled model: the canonical EOS section (α^r, α⁰, R, reducing state), M and the
    /// name (E14).
    pub fn model_key(&self) -> ModelKey {
        let mut bytes = Vec::new();
        self.eos.encode(&mut bytes);
        bytes.extend_from_slice(&self.molar_mass.to_le_bytes());
        bytes.extend_from_slice(self.name.as_bytes());
        ModelKey::from_content(&bytes)
    }

    /// A package builder (the registry's decoder adds the lazy saturation and transport parts).
    pub fn builder(self) -> Result<PureFluidBuilder, Error> {
        let e = &self.eos;
        let pending = [
            (e.lemmon2005.is_empty(), "Lemmon2005", "M3.3"),
            (e.double_exponential.is_empty(), "DoubleExponential", "M3.3"),
            (e.gaussian.is_empty(), "Gaussian", "M3.4"),
            (e.gao_b.is_empty(), "GaoB", "M3.5"),
            (e.non_analytic.is_empty(), "NonAnalytic", "M4.1"),
        ];
        if let Some((_, kind, step)) = pending.into_iter().find(|(empty, ..)| !empty) {
            return Err(Error::Load(LoadError::Format(format!("{kind} terms land at {step}").into())));
        }
        let ideal = IdealGas::new(e.t_reducing, e.rho_reducing, e.ideal.clone())?;
        let blocks = vec![ResidualBlock::Power(PowerBlock::new(&e.power)?)];
        let eos = MultiParameterEos::new(e.gas_constant, e.t_reducing, e.rho_reducing, e.rho_max, blocks, ideal)?;
        let aliases: Vec<&str> = self.aliases.iter().map(String::as_str).collect();
        let info = FluidInfo::new(&self.name, self.molar_mass, self.source.clone(), self.model_key())?;
        let builder = PureFluid::builder(info.with_aliases(&aliases), eos, self.limits);
        Ok(match self.critical {
            Some(c) => builder.critical(c),
            None => builder,
        })
    }

    /// Compiles into an immutable package without lazy parts.
    pub fn compile(self) -> Result<PureFluid, Error> {
        Ok(self.builder()?.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FlashOptions;
    use crate::fluid::Fluid;
    use crate::input::Input;
    use crate::model::ThermoModel;
    use crate::units::{Density, Temperature};

    fn record() -> FluidRecord {
        let mut r = FluidRecord::toy("R1234ze(E)").unwrap();
        r.corrections = vec![Patch { divergence: "DIV-0001".into(), edit: Edit::GasConstant(8.314_462_1) }];
        let e = &r.eos;
        r.superancillary_fit =
            Some(SaStamp { shape: e.shape_hash(), gas_constant: e.gas_constant, rho_reducing: e.rho_reducing });
        r
    }

    /// Parity is bit-exact v8.0.0; Corrected differs by exactly the patch. An R correction rescales the
    /// superancillary exactly (E14) instead of demoting it.
    #[test]
    fn parity_and_corrected_differ_by_exactly_the_patch() {
        let mut parity = record();
        parity.apply(DataSet::Parity).unwrap();
        let mut corrected = record();
        corrected.apply(DataSet::Corrected).unwrap();
        assert_eq!(parity, record());
        assert_eq!(corrected.applied, vec![Box::<str>::from("DIV-0001")]);
        let mut undone = corrected.clone();
        (undone.eos.gas_constant, undone.applied) = (parity.eos.gas_constant, vec![]);
        assert_eq!(undone, parity);
        assert_eq!(parity.superancillary_freshness(), Some(SaFreshness::Fresh));
        let p = 8.314_462_1 / 8.314_472;
        assert_eq!(corrected.superancillary_freshness(), Some(SaFreshness::Rescaled { p, rho: 1.0 }));
        let mut reshaped = parity.clone();
        reshaped.eos.power[1].n = 0.51;
        assert_eq!(reshaped.superancillary_freshness(), Some(SaFreshness::Stale));

        // Same (T, ρ): p scales with R exactly (p = ρRT·A01; A01 does not depend on R).
        let input = Input::dt(Density::molar(5000.0).unwrap(), Temperature::new(300.0).unwrap());
        let opts = FlashOptions::new().with_phase(crate::Phase::Liquid);
        let (a, b) = (parity.compile().unwrap(), corrected.compile().unwrap());
        assert_ne!(a.info().key(), b.info().key());
        let (a, b) = (Fluid::new(Arc::new(a)), Fluid::new(Arc::new(b)));
        let (pa, pb) = (a.flash(input, &opts).unwrap().p(), b.flash(input, &opts).unwrap().p());
        assert!(((pb / pa) - p).abs() < 1e-15);
    }

    /// The toy record plus one term of every kind the evaluator does not compile yet.
    fn every_kind() -> FluidRecord {
        let mut r = FluidRecord::toy("X").unwrap();
        let e = &mut r.eos;
        e.lemmon2005 = vec![Lemmon2005Term { n: 0.1, t: 0.2, d: 1, l: 2, m: 0.3 }];
        e.double_exponential = vec![DoubleExponentialTerm { n: 0.1, t: 0.2, d: 1, gd: 0.3, ld: 2, gt: -0.4, lt: 1.0 }];
        e.gaussian = vec![GaussianTerm { n: 0.1, t: 0.2, d: 1, eta: 0.3, epsilon: 0.4, beta: 0.5, gamma: 0.6 }];
        e.gao_b = vec![GaoBTerm { n: 0.1, t: 0.2, d: 1, eta: 0.3, epsilon: 0.4, beta: 0.5, gamma: 0.6, b: 0.7 }];
        e.non_analytic =
            vec![NonAnalyticTerm { n: 0.1, a: 0.2, b: 0.3, beta: 0.4, big_a: 0.5, big_b: 0.6, big_c: 0.7, big_d: 0.8 }];
        e.ideal.extend([
            IdealTerm::PlanckEinsteinGeneralized { n: 0.1, theta: 0.2, c: 0.3, d: 0.4 },
            IdealTerm::Cp0Power { c: 0.1, t: 0.2, tc: 0.3, t0: 0.4 },
            IdealTerm::Offset { a1: 0.1, a2: 0.2, reference: OffsetReference::Iir },
        ]);
        r
    }

    /// A residual kind without an evaluator is refused when the record compiles, never dropped from α^r.
    #[test]
    fn kinds_without_an_evaluator_are_refused() {
        type Clear = fn(&mut EosRecord);
        let pending: [(Clear, &str); 5] = [
            (|e| e.lemmon2005.clear(), "Lemmon2005 terms land at M3.3"),
            (|e| e.double_exponential.clear(), "DoubleExponential terms land at M3.3"),
            (|e| e.gaussian.clear(), "Gaussian terms land at M3.4"),
            (|e| e.gao_b.clear(), "GaoB terms land at M3.5"),
            (|e| e.non_analytic.clear(), "NonAnalytic terms land at M4.1"),
        ];
        let mut record = every_kind();
        for (clear, why) in pending {
            let err = record.clone().compile().unwrap_err().to_string();
            assert!(err.contains(why), "{err}");
            clear(&mut record.eos);
        }
        let err = record.clone().compile().unwrap_err().to_string();
        assert!(err.contains("ideal-gas PlanckEinsteinGeneralized terms land at M4.2"), "{err}");
        record.eos.ideal.truncate(2);
        assert!(record.compile().is_ok());
    }

    /// E14, as an exhaustive property over the toy record with one term of every kind: changing any EOS field flips
    /// the EOS hash and the model key; the shape hash ignores exactly R, ρ_r and ρ_max; M changes the key but not the
    /// EOS hash.
    #[test]
    fn every_eos_field_is_hashed() {
        let base = every_kind();
        type Mutation = (&'static str, fn(&mut FluidRecord), bool);
        let mutations: Vec<Mutation> = vec![
            ("R", |r| r.eos.gas_constant *= 1.0 + 1e-15, false),
            ("rho_r", |r| r.eos.rho_reducing += 1e-9, false),
            ("rho_max", |r| r.eos.rho_max += 1.0, false),
            ("T_r", |r| r.eos.t_reducing += 1e-9, true),
            ("n", |r| r.eos.power[0].n += 1e-12, true),
            ("t", |r| r.eos.power[0].t += 1e-12, true),
            ("d", |r| r.eos.power[0].d += 1, true),
            ("l", |r| r.eos.power[1].l += 1, true),
            ("c", |r| r.eos.power[1].c += 1e-12, true),
            ("ideal a1", |r| r.eos.ideal[0] = IdealTerm::Lead { a1: -12.5 + 1e-12, a2: 8.6 }, true),
            ("ideal kind", |r| r.eos.ideal[1] = IdealTerm::Power { n: 3.0, t: 0.0 }, true),
            ("extra term", |r| r.eos.power.push(PowerTerm::new(0.0, 0.0, 0, 0, 0.0)), true),
            ("lemmon n", |r| r.eos.lemmon2005[0].n += 1e-12, true),
            ("lemmon t", |r| r.eos.lemmon2005[0].t += 1e-12, true),
            ("lemmon d", |r| r.eos.lemmon2005[0].d += 1, true),
            ("lemmon l", |r| r.eos.lemmon2005[0].l += 1, true),
            ("lemmon m", |r| r.eos.lemmon2005[0].m += 1e-12, true),
            ("double n", |r| r.eos.double_exponential[0].n += 1e-12, true),
            ("double t", |r| r.eos.double_exponential[0].t += 1e-12, true),
            ("double d", |r| r.eos.double_exponential[0].d += 1, true),
            ("double gd", |r| r.eos.double_exponential[0].gd += 1e-12, true),
            ("double ld", |r| r.eos.double_exponential[0].ld += 1, true),
            ("double gt", |r| r.eos.double_exponential[0].gt += 1e-12, true),
            ("double lt", |r| r.eos.double_exponential[0].lt += 1e-12, true),
            ("gaussian n", |r| r.eos.gaussian[0].n += 1e-12, true),
            ("gaussian t", |r| r.eos.gaussian[0].t += 1e-12, true),
            ("gaussian d", |r| r.eos.gaussian[0].d += 1, true),
            ("gaussian eta", |r| r.eos.gaussian[0].eta += 1e-12, true),
            ("gaussian epsilon", |r| r.eos.gaussian[0].epsilon += 1e-12, true),
            ("gaussian beta", |r| r.eos.gaussian[0].beta += 1e-12, true),
            ("gaussian gamma", |r| r.eos.gaussian[0].gamma += 1e-12, true),
            ("gaob n", |r| r.eos.gao_b[0].n += 1e-12, true),
            ("gaob t", |r| r.eos.gao_b[0].t += 1e-12, true),
            ("gaob d", |r| r.eos.gao_b[0].d += 1, true),
            ("gaob eta", |r| r.eos.gao_b[0].eta += 1e-12, true),
            ("gaob epsilon", |r| r.eos.gao_b[0].epsilon += 1e-12, true),
            ("gaob beta", |r| r.eos.gao_b[0].beta += 1e-12, true),
            ("gaob gamma", |r| r.eos.gao_b[0].gamma += 1e-12, true),
            ("gaob b", |r| r.eos.gao_b[0].b += 1e-12, true),
            ("nonanalytic n", |r| r.eos.non_analytic[0].n += 1e-12, true),
            ("nonanalytic a", |r| r.eos.non_analytic[0].a += 1e-12, true),
            ("nonanalytic b", |r| r.eos.non_analytic[0].b += 1e-12, true),
            ("nonanalytic beta", |r| r.eos.non_analytic[0].beta += 1e-12, true),
            ("nonanalytic A", |r| r.eos.non_analytic[0].big_a += 1e-12, true),
            ("nonanalytic B", |r| r.eos.non_analytic[0].big_b += 1e-12, true),
            ("nonanalytic C", |r| r.eos.non_analytic[0].big_c += 1e-12, true),
            ("nonanalytic D", |r| r.eos.non_analytic[0].big_d += 1e-12, true),
            (
                "generalized",
                |r| r.eos.ideal[2] = IdealTerm::PlanckEinsteinGeneralized { n: 0.1, theta: 0.2, c: 0.3, d: 0.5 },
                true,
            ),
            ("cp0 tc", |r| r.eos.ideal[3] = IdealTerm::Cp0Power { c: 0.1, t: 0.2, tc: 0.31, t0: 0.4 }, true),
            ("cp0 t0", |r| r.eos.ideal[3] = IdealTerm::Cp0Power { c: 0.1, t: 0.2, tc: 0.3, t0: 0.41 }, true),
            (
                "offset a2",
                |r| r.eos.ideal[4] = IdealTerm::Offset { a1: 0.1, a2: 0.21, reference: OffsetReference::Iir },
                true,
            ),
            (
                "offset tag",
                |r| r.eos.ideal[4] = IdealTerm::Offset { a1: 0.1, a2: 0.2, reference: OffsetReference::Custom },
                true,
            ),
        ];
        for (what, mutate, shape) in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            assert_ne!(m.eos.eos_hash(), base.eos.eos_hash(), "{what}");
            assert_ne!(m.model_key(), base.model_key(), "{what}");
            assert_eq!(m.eos.shape_hash() != base.eos.shape_hash(), shape, "{what}");
        }
        let mut heavier = base.clone();
        heavier.molar_mass += 1e-9;
        assert_eq!(heavier.eos.eos_hash(), base.eos.eos_hash());
        assert_ne!(heavier.model_key(), base.model_key());
    }
}
