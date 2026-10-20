//! What the diagram tests share: the fluids, the arbiter (a vertex is the flash's value at its inputs, bit for bit) and
//! a batch that fails cells on purpose.
#![allow(dead_code)] // each test binary uses its own part

use phasekit_core::{Error, Fluid, Pair, Registry};
use phasekit_diagram::{Batch, FluidBatch, Plane, Quantity, Row, Vertex};

/// The five embedded fluids, and the pseudo-pure Air and R410A, whose domes have bubble and dew sides.
pub const FLUIDS: [&str; 7] = ["Water", "n-Propane", "CarbonDioxide", "Nitrogen", "R134a", "Air", "R410A"];

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

/// Fails every cell of row `i`.
pub fn fail_row(rows: &mut [Row], i: usize) {
    if let Some(row) = rows.get_mut(i) {
        row.ok.iter_mut().for_each(|ok| *ok = false);
    }
}
