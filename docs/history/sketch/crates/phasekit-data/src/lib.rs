//! Generated fluid data: one versioned little-endian blob per fluid plus the name/alias/CAS index (D7).
//!
//! No logic lives here, so data churn (new corrections, refits) is versioned apart from the kernel, and the
//! CoolProp MIT notice has a clean boundary (D14). `xtask datagen` writes this file; in the real crate each
//! blob is `include_bytes!` of `blobs/<fluid>.bin`. The sketch uses short placeholders.
//!
//! Every fluid is always *listed* (the index costs ~7 KB), but only fluids whose feature is on carry bytes.
//! The core registry indexes only fluids with bytes; for a listed-but-absent name it answers
//! `LoadError::NotEmbedded { feature }` without caching, so a later runtime pack can still supply it (E6).
#![no_std]

/// Identifier of the dataset these blobs were generated from (recorded in fixtures and `Source`).
pub const DATASET: &str = "coolprop-8.0.0+ae81610e";

/// One fluid in the embedded index.
#[derive(Clone, Copy, Debug)]
pub struct FluidEntry {
    /// Canonical CoolProp name.
    pub name: &'static str,
    /// Aliases and CAS number, as listed by CoolProp 8.0.0.
    pub aliases: &'static [&'static str],
    /// Canonical names of the fluids this one's lazy parts need (ECS transport reference fluids; map 05
    /// §5). Declared here so the registry resolves them when it is built, never at first use (E5).
    pub requires: &'static [&'static str],
    /// The Cargo feature that embeds this fluid's bytes.
    pub feature: &'static str,
    /// The encoded blob; empty when the feature is off.
    pub blob: &'static [u8],
}

macro_rules! blob {
    ($feature:literal, $bytes:literal) => {{
        #[cfg(feature = $feature)]
        let b: &[u8] = $bytes;
        #[cfg(not(feature = $feature))]
        let b: &[u8] = &[];
        b
    }};
}

/// The embedded index (generated; 136 entries in the real crate).
pub static FLUIDS: &[FluidEntry] = &[
    FluidEntry {
        name: "Water",
        aliases: &["H2O", "R718", "7732-18-5"],
        requires: &[],
        feature: "fluid-water",
        blob: blob!("fluid-water", b"PKIT\x00placeholder:water"),
    },
    FluidEntry {
        name: "Nitrogen",
        aliases: &["N2", "R728", "7727-37-9"],
        requires: &[],
        feature: "fluid-nitrogen",
        blob: blob!("fluid-nitrogen", b"PKIT\x00placeholder:nitrogen"),
    },
    FluidEntry {
        name: "R134a",
        aliases: &["811-97-2"],
        requires: &[],
        feature: "fluid-r134a",
        blob: blob!("fluid-r134a", b"PKIT\x00placeholder:r134a"),
    },
    FluidEntry {
        name: "Propane",
        aliases: &["n-Propane", "R290", "C3H8", "nC3H8", "n-C3H8", "74-98-6"],
        requires: &[],
        feature: "fluid-propane",
        blob: blob!("fluid-propane", b"PKIT\x00placeholder:propane"),
    },
    FluidEntry {
        name: "R143a",
        aliases: &["420-46-2"],
        requires: &["R134a"],
        feature: "fluid-r143a",
        blob: blob!("fluid-r143a", b"PKIT\x00placeholder:r143a"),
    },
];
