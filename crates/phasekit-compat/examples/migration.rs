//! docs/MIGRATION.md's Rust example: CoolProp's `PropsSI` call through compat, then the same state through the typed
//! API. `cargo run -p phasekit-compat --example migration`.

#![allow(clippy::print_stdout, reason = "an example prints its result")]

use phasekit_core::{Basis, FlashOptions, Input, Pressure, Quality, Registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CoolProp: PropsSI("T", "P", 101325, "Q", 0, "Water")
    let t = phasekit_compat::props_si("T", "P", 101_325.0, "Q", 0.0, "Water")?;

    // The typed API: one flash, then any property of the state.
    let water = Registry::embedded()?.get("Water")?;
    let state = water.flash(Input::pq(Pressure::new(101_325.0)?, Quality::new(0.0)?), &FlashOptions::new())?;
    assert_eq!(state.t(), t);
    println!("T = {t} K, h = {} J/kg, s = {} J/(kg K)", state.h(Basis::Mass), state.s(Basis::Mass));
    Ok(())
}
