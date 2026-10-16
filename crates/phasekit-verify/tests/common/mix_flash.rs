//! A mixture's PT equilibria (PLAN.md M13.8): the oracle's `mix_flash` rows, the binaries they belong to, and each
//! phase's fugacities and Gibbs energy on its stable root.

#![allow(dead_code, reason = "each test binary that includes the rows uses its part of them")]

use phasekit_core::{DataSet, Fluid, Order, Registry, math};
use phasekit_mix::{Equilibrium, Mixture};
use phasekit_verify::{Fixture, ToleranceClass, fixture};

/// One of `mix_flash`'s PT flashes.
pub struct FlashRow {
    /// The binary, "A&B".
    pub label: String,
    /// The first component's mole fraction.
    pub z1: f64,
    /// T (K) and p (Pa), the inputs.
    pub t: f64,
    pub p: f64,
    /// Q (−1 for one phase), the bulk molar density, and a split's first liquid and vapour fractions (NaN otherwise).
    pub q: f64,
    pub rho: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Every row the oracle flashed (`status` ok), in the file's order.
pub fn flash_rows() -> Vec<FlashRow> {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_flash.csv");
    let table = Fixture::parse(path, text).unwrap();
    (0..table.rows().len())
        .filter(|&row| table.printed(row, "status") == Some("ok"))
        .map(|row| {
            let value = |column: &str| table.value(row, column).unwrap();
            FlashRow {
                label: table.printed(row, "mixture").unwrap().to_string(),
                z1: value("z1"),
                t: value("T"),
                p: value("p"),
                q: value("Q"),
                rho: value("rhomolar"),
                x1: value("x1"),
                y1: value("y1"),
            }
        })
        .collect()
}

/// The binary `label` at `z1` on `Parity`, the oracle's data.
pub fn binary(registry: &Registry, label: &str, z1: f64) -> Fluid {
    let names: Vec<&str> = label.split('&').collect();
    phasekit_mix::mixture(registry, &[(names[0], z1), (names[1], 1.0 - z1)], DataSet::Parity).unwrap()
}

/// The mixture model behind `fluid`.
pub fn mixture(fluid: &Fluid) -> &Mixture {
    (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>().unwrap()
}

/// ln φ_i of mole fractions `x` of `mixture`'s components at (T, ρ).
pub fn ln_phi(mixture: &Mixture, t: f64, rho: f64, x: &[f64]) -> Vec<f64> {
    use phasekit_mix::MixtureModel;
    let c = mixture.model().composition(t, rho, x, Order::One).unwrap();
    (0..x.len()).map(|i| c.ln_phi(i)).collect()
}

/// The density of mole fractions `x` at (T, p) on its stable root, as the PT flash selects it.
pub fn root(registry: &Registry, mixture: &Mixture, t: f64, p: f64, x: &[f64]) -> f64 {
    let names: Vec<String> = mixture.model().fluids().map(|f| f.info().name().to_string()).collect();
    let parts: Vec<(&str, f64)> = names.iter().map(String::as_str).zip(x.iter().copied()).collect();
    let fluid = phasekit_mix::mixture(registry, &parts, DataSet::Parity).unwrap();
    phasekit_mix::internal::stable_root(fluid.model().helmholtz().unwrap(), t, p).unwrap()
}

/// ln f_i^V − ln f_i^L of a split and the bound `Identity` puts on each (1e-12 of its terms' largest).
pub fn fugacity_differences(mixture: &Mixture, t: f64, split: &Equilibrium) -> Vec<(f64, f64)> {
    let Equilibrium::Split { x, y, rho_liquid, rho_vapour, .. } = split else { return Vec::new() };
    let (phi_l, phi_v) = (ln_phi(mixture, t, *rho_liquid, x), ln_phi(mixture, t, *rho_vapour, y));
    (0..x.len())
        .map(|i| {
            let terms = [math::ln(y[i]), phi_v[i], math::ln(x[i]), phi_l[i]];
            let bound = ToleranceClass::Identity.bound(terms.iter().fold(1.0_f64, |m, t| m.max(t.abs()))).unwrap();
            (terms[0] + terms[1] - terms[2] - terms[3], bound)
        })
        .collect()
}

/// G/RT relative to the pure ideal gases at (T, p) of mole fractions `x` at density `rho`: Σ x_i ln(x_i φ_i).
pub fn gibbs(mixture: &Mixture, t: f64, rho: f64, x: &[f64]) -> f64 {
    ln_phi(mixture, t, rho, x).iter().zip(x).map(|(l, x)| x * (math::ln(*x) + l)).sum()
}

/// A split's vapour fraction β, liquid and vapour fractions, and liquid and vapour densities.
pub type Parts<'a> = (f64, &'a [f64], &'a [f64], f64, f64);

/// A split's parts; `None` for one phase.
pub fn parts(e: &Equilibrium) -> Option<Parts<'_>> {
    match e {
        Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour } => Some((*beta, x, y, *rho_liquid, *rho_vapour)),
        _ => None,
    }
}

/// A single phase's density; `None` for a split.
pub fn single(e: &Equilibrium) -> Option<f64> {
    match e {
        Equilibrium::Single { rho } => Some(*rho),
        _ => None,
    }
}
