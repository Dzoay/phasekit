//! Humid air's viscosity and thermal conductivity (PLAN.md M18.5; user decision HA3): Tsilingiris's mixing rule as
//! v8.0.0's `Viscosity` and `Conductivity` write it (map 07 §3.3), Wilke's interaction factors from the two parts' own
//! viscosities,
//! Φ_av = √2/4·(1 + M_a/M_w)^(−1/2)·(1 + (μ_a/μ_w)^(1/2)·(M_w/M_a)^(1/4))², Φ_va likewise with a and v swapped,
//! μ = (1 − ψ)μ_a/((1 − ψ) + ψΦ_av) + ψμ_w/(ψ + (1 − ψ)Φ_va), and k the same with the parts' k in place of their μ.
//! Dry air's μ and k are the air part's at (T, p); water vapour's the water part's as saturated vapour at the total
//! pressure, which does not depend on T (map 07 H2); M_a and M_w the parts' own molar masses. Both data sets take them
//! so until Tsilingiris (2008) is in hand (PLAN.md §6; ROT-262 stays deferred), so the oracle is the only check:
//! provisional.

use phasekit_core::{Error, Fluid, Input, Pressure, Prop, Quality, State, math};

use crate::saturation::at;
use crate::{HumidAir, MoistAir};

/// μ and k of a part at `state`.
fn transport(part: &Fluid, state: &State) -> Result<[f64; 2], Error> {
    Ok([part.prop(state, Prop::Viscosity)?, part.prop(state, Prop::Conductivity)?])
}

/// Wilke's interaction factor of `a` with `b` from their viscosities and molar masses.
fn phi(mu_a: f64, mu_b: f64, m_a: f64, m_b: f64) -> f64 {
    let wing = 1.0 + math::sqrt(mu_a / mu_b) * math::powf(m_b / m_a, 0.25);
    math::sqrt(2.0) / 4.0 / math::sqrt(1.0 + m_a / m_b) * wing * wing
}

/// μ (Pa s) and k (W/(m K)) of `state` (see the module's notes).
pub(crate) fn mixed(air: &HumidAir, state: &MoistAir) -> Result<[f64; 2], Error> {
    let parts = air.parts();
    let dry = at(&parts.air, state.t(), state.p())?;
    let vapour = parts.water.state(Input::pq(Pressure::new(state.p())?, Quality::new(1.0)?))?;
    let ([mu_a, k_a], [mu_w, k_w]) = (transport(&parts.air, &dry)?, transport(&parts.water, &vapour)?);
    let (m_a, m_w) = (parts.air.info().molar_mass(), parts.water.info().molar_mass());
    let (phi_av, phi_va) = (phi(mu_a, mu_w, m_a, m_w), phi(mu_w, mu_a, m_w, m_a));
    let (psi, x) = (state.mole_fraction(), 1.0 - state.mole_fraction());
    let mix = |a: f64, w: f64| x * a / (x + psi * phi_av) + psi * w / (psi + x * phi_va);
    Ok([mix(mu_a, mu_w), mix(k_a, k_w)])
}

impl HumidAir {
    /// The viscosity of `state`, Pa s (see the module's notes).
    pub fn viscosity(&self, state: &MoistAir) -> Result<f64, Error> {
        Ok(mixed(self, state)?[0])
    }

    /// The thermal conductivity of `state`, W/(m K) (see the module's notes).
    pub fn conductivity(&self, state: &MoistAir) -> Result<f64, Error> {
        Ok(mixed(self, state)?[1])
    }
}
