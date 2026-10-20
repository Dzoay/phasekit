//! What the diagram tests share: the fluids, the arbiter (a vertex is the flash's value at its inputs, bit for bit) and
//! a batch that fails cells on purpose.
#![allow(dead_code)] // each test binary uses its own part

use phasekit_core::{Error, Fluid, Pair, Registry};
use phasekit_diagram::{Batch, FluidBatch, Limits, Plane, Quantity, Row, Vertex};

/// The five embedded fluids, and the pseudo-pure Air and R410A, whose domes have bubble and dew sides.
pub const FLUIDS: [&str; 7] = ["Water", "n-Propane", "CarbonDioxide", "Nitrogen", "R134a", "Air", "R410A"];

/// The fluids and planes the isoline tests run on: every plane, and both pseudo-pure fluids.
pub const CASES: [(&str, Plane); 8] = [
    ("Water", Plane::Ph),
    ("Water", Plane::Ts),
    ("Water", Plane::Hs),
    ("n-Propane", Plane::Ph),
    ("n-Propane", Plane::TRho),
    ("CarbonDioxide", Plane::Pt),
    ("R410A", Plane::Ph),
    ("Air", Plane::Ts),
];

/// The plotted T and p (PLAN.md M19.1): T from the greater of T_min and T_triple to the lesser of T_max and 1.6 T_c,
/// p from the greater of p_triple and p_c/10⁴ to the lesser of P_max and 10 p_c.
pub fn plotted_range(l: &Limits) -> ((f64, f64), (f64, f64)) {
    let t = (l.t_min.max(l.t_triple), l.t_max.min(1.6 * l.t_critical));
    (t, (l.p_triple.max(l.p_critical / 1e4), l.p_max.min(l.p_critical * 10.0)))
}

/// A fluid of the embedded registry.
pub fn fluid(name: &str) -> Result<&'static Fluid, Error> {
    Registry::embedded()?.get(name)
}

/// The value of `quantity` at the state of the vertex's inputs, by one flash.
pub fn flashed(fluid: &Fluid, quantity: Quantity, v: &Vertex) -> Result<f64, Error> {
    let state = fluid.state(v.inputs.input()?)?;
    fluid.prop(&state, quantity.prop())
}

/// Whether the vertex's x and y are, bit for bit, the flash's at its inputs (`Exact`).
pub fn is_flash(fluid: &Fluid, plane: Plane, v: &Vertex) -> Result<bool, Error> {
    let x = flashed(fluid, plane.x().quantity, v)?;
    let y = flashed(fluid, plane.y().quantity, v)?;
    Ok(x.to_bits() == v.x.to_bits() && y.to_bits() == v.y.to_bits())
}

/// A fluid's batch whose answer `change` alters wherever `when` holds of the call.
pub struct Tamper<'a, W, C> {
    pub inner: FluidBatch<'a>,
    pub when: W,
    pub change: C,
}

impl<W, C> Batch for Tamper<'_, W, C>
where
    W: Fn(&[Quantity], Pair, &[f64], &[f64]) -> bool,
    C: Fn(&mut [Row]),
{
    fn rows(&self, outputs: &[Quantity], pair: Pair, x: &[f64], y: &[f64]) -> Result<Vec<Row>, Error> {
        let mut rows = self.inner.rows(outputs, pair, x, y)?;
        if (self.when)(outputs, pair, x, y) {
            (self.change)(&mut rows);
        }
        Ok(rows)
    }
}

/// Fails every cell of the row.
pub fn fail(row: &mut Row) {
    row.ok.iter_mut().for_each(|ok| *ok = false);
}
