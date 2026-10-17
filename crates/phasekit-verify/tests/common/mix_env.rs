//! A mixture's phase envelope (PLAN.md M13.12): the oracle's `mix_env` points.

#![allow(dead_code, reason = "each test binary that includes the rows uses its part of them")]

use phasekit_core::math;
use phasekit_verify::{Fixture, fixture};

/// One of `mix_env`'s points.
pub struct EnvRow {
    /// The binary, "A&B".
    pub label: String,
    /// The first component's mole fraction.
    pub z1: f64,
    /// Q: 0 at a bubble point (the feed the liquid), 1 at a dew point (the feed the vapour).
    pub q: f64,
    /// T (K) and p (Pa).
    pub t: f64,
    pub p: f64,
    /// The incipient phase's and the feed's molar densities, and the incipient phase's first fraction.
    pub rho_incipient: f64,
    pub rho_bulk: f64,
    pub x1: f64,
}

/// Every point the oracle traced (`status` ok), in the file's order.
pub fn env_rows() -> Vec<EnvRow> {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_env.csv");
    let table = Fixture::parse(path, text).unwrap();
    (0..table.rows().len())
        .filter(|&row| table.printed(row, "status") == Some("ok"))
        .map(|row| {
            let value = |column: &str| table.value(row, column).unwrap();
            EnvRow {
                label: table.printed(row, "mixture").unwrap().to_string(),
                z1: value("z1"),
                q: value("Q"),
                t: value("T"),
                p: value("p"),
                rho_incipient: value("rho_incipient"),
                rho_bulk: value("rho_bulk"),
                x1: value("x1"),
            }
        })
        .collect()
}

impl EnvRow {
    /// Whether the point's two phases lie within 0.1 of each other in |ln ρ|, near the critical point (DIV-0063).
    pub fn near_critical(&self) -> bool {
        math::ln(self.rho_incipient / self.rho_bulk).abs() < 0.1
    }
}
