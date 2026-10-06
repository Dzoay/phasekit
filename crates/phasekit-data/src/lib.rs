//! Generated fluid data: one versioned little-endian blob per fluid plus the name/alias/CAS index (D7).
//!
//! No logic lives here, so data churn (new corrections, refits) is versioned apart from the kernel, and the
//! CoolProp MIT notice has a clean boundary (D14; NOTICE). `cargo xtask datagen` writes `generated.rs`, the blobs
//! under `blobs/` and this crate's features; `cargo xtask gates datagen` checks that the committed files are exactly
//! what it generates.
//!
//! Every fluid is always *listed* (the index costs a few KB), but only fluids whose feature is on carry bytes.
//! The core registry indexes only fluids with bytes; for a listed-but-absent name it answers
//! `LoadError::NotEmbedded { feature }` without caching, so a later runtime pack can still supply it (E6).
#![no_std]

/// One fluid in the embedded index.
#[derive(Clone, Copy, Debug)]
pub struct FluidEntry {
    /// Canonical CoolProp name.
    pub name: &'static str,
    /// Aliases, CAS number and InChIKey as CoolProp 8.0.0 lists them, each ASCII case-folded key once.
    pub aliases: &'static [&'static str],
    /// Canonical names of the fluids this one's lazy parts need (ECS transport reference fluids; map 05
    /// §5). Declared here so the registry resolves them when it is built, never at first use (E5).
    pub requires: &'static [&'static str],
    /// The Cargo feature that embeds this fluid's bytes.
    pub feature: &'static str,
    /// The encoded blob (format v1); empty when the feature is off.
    pub blob: &'static [u8],
}

/// The blob `blobs/<file>` when `feature` is on, else empty.
macro_rules! blob {
    ($feature:literal, $file:literal) => {{
        #[cfg(feature = $feature)]
        let b: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/blobs/", $file));
        #[cfg(not(feature = $feature))]
        let b: &[u8] = &[];
        b
    }};
}

#[rustfmt::skip] // written by `cargo xtask datagen`, compared byte for byte by `gates datagen`
mod generated;

pub use generated::{DATASET, FLUIDS};
