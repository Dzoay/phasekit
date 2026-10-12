//! Datagen's tests, one module per stage: the pinned files read ([`files`]), each EOS mapped into core's record
//! ([`mapping`]), the blobs ([`blobs`]), the ideal-gas kinds as converted ([`ideal`]) and the caloric curves
//! ([`caloric_curves`]); the fixtures they share are here.

use super::*;
use mirror::{IdealBlock, Num, ResidualBlock};
use phasekit_core::internal::{Environmental, EosRecord, FluidRecord, IdealTerm};
use phasekit_core::{Jet4, Limits, Order, Real, math};

mod blobs;
mod caloric_curves;
mod files;
mod ideal;
mod mapping;

fn sources() -> Vec<Source> {
    load(&Repo::locate()).unwrap()
}

/// The text of one pinned fluid file.
fn text(file: &str) -> String {
    Repo::locate().read(&format!("{FLUIDS_DIR}/{file}")).unwrap()
}

/// An edit of a parsed fluid file.
type Edit = fn(&mut serde_json::Value);

/// A pinned file with one edit applied to its parsed JSON, written back as JSON (numbers keep their literal kind).
fn edited(file: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&text(file)).unwrap();
    edit(&mut value);
    serde_json::to_string(&value).unwrap()
}

/// Every fluid's record, by name.
fn records() -> Vec<(Source, FluidRecord)> {
    sources()
        .into_iter()
        .map(|s| {
            let r = record::to_record(&s).unwrap();
            (s, r)
        })
        .collect()
}

fn record_of(file: &str, text: &str) -> Result<FluidRecord, String> {
    record::to_record(&parse(file, text)?)
}
