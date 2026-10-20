//! The batch the geometry samples (PLAN.md M19.1): a fluid's, through `phasekit_core::batch::evaluate`, or any other
//! that answers as it does, so that a test can fail a cell on purpose. A cell is a value where its status is `Ok` or
//! `Extrapolated` (a flagged value, PLAN.md M10.8); any other status leaves a gap.

use phasekit_core::batch::{BatchRequest, ExecPolicy, Status, evaluate};
use phasekit_core::{Basis, Error, FlashOptions, Fluid, Input, Pair, Phase, Prop};

use crate::plane::Quantity;

/// The inputs a vertex was computed at: an input pair and its two values in the pair's order, D, H and S per unit mass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inputs {
    /// The input pair.
    pub pair: Pair,
    /// The pair's first value.
    pub x: f64,
    /// The pair's second value.
    pub y: f64,
}

impl Inputs {
    /// The input these values name, for a flash.
    pub fn input(&self) -> Result<Input, Error> {
        Input::new(self.pair, self.x, self.y, Basis::Mass)
    }
}

/// A point of a batch: each output's value, whether it is one, and the point's inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The outputs' values, in the order asked; NaN where a cell failed.
    pub cells: Vec<f64>,
    /// Whether each cell is a value.
    pub ok: Vec<bool>,
    /// The point's inputs.
    pub inputs: Inputs,
}

/// A point of a line or of the dome: its plane's x and y at the state of its inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// The value of the plane's horizontal quantity.
    pub x: f64,
    /// The value of the plane's vertical quantity.
    pub y: f64,
    /// The inputs of the state.
    pub inputs: Inputs,
}

/// A batch over one fluid.
pub trait Batch {
    /// One row a point: `outputs` at the states of `pair` at `x[i]` and `y[i]`, in the pair's order, D, H and S per
    /// unit mass. A failed cell is its row's `ok` false; an error refuses the whole call.
    fn rows(&self, outputs: &[Quantity], pair: Pair, x: &[f64], y: &[f64]) -> Result<Vec<Row>, Error>;
}

/// A fluid's batch, under its flash options and executor (by default `FlashOptions::new()` and `Reference`).
#[derive(Clone, Copy, Debug)]
pub struct FluidBatch<'a> {
    fluid: &'a Fluid,
    flash: FlashOptions,
    exec: ExecPolicy,
}

impl<'a> FluidBatch<'a> {
    /// The fluid's batch under the default options and the `Reference` executor.
    pub fn new(fluid: &'a Fluid) -> Self {
        Self { fluid, flash: FlashOptions::new(), exec: ExecPolicy::Reference }
    }

    /// Flash options for every point.
    pub const fn with_flash(mut self, flash: FlashOptions) -> Self {
        self.flash = flash;
        self
    }

    /// The executor.
    pub const fn with_exec(mut self, exec: ExecPolicy) -> Self {
        self.exec = exec;
        self
    }
}

impl FluidBatch<'_> {
    /// The library's phase at the state of `inputs`, under the batch's flash options; `None` where it refuses the
    /// state (PLAN.md M19.5).
    pub fn phase(&self, inputs: Inputs) -> Option<Phase> {
        Some(self.fluid.flash(inputs.input().ok()?, &self.flash).ok()?.phase())
    }
}

impl Batch for FluidBatch<'_> {
    fn rows(&self, outputs: &[Quantity], pair: Pair, x: &[f64], y: &[f64]) -> Result<Vec<Row>, Error> {
        let props: Vec<Prop> = outputs.iter().map(|q| q.prop()).collect();
        let request = BatchRequest::new(pair, Basis::Mass, x, y, &props).with_flash(self.flash).with_exec(self.exec);
        let cells = request.cells().ok_or(Error::Shape { expected: usize::MAX, found: x.len() })?;
        let (mut values, mut status) = (vec![0.0; cells], vec![Status::Ok; cells]);
        evaluate(self.fluid, &request, &mut values, &mut status)?;
        Ok(split(pair, x, y, &values, &status, outputs.len()))
    }
}

/// Point-major buffers of `width` cells a point as one row a point.
fn split(pair: Pair, x: &[f64], y: &[f64], values: &[f64], status: &[Status], width: usize) -> Vec<Row> {
    let at = |i: usize| i.saturating_mul(width)..i.saturating_add(1).saturating_mul(width);
    let row = |(i, (&x, &y)): (usize, (&f64, &f64))| Row {
        cells: values.get(at(i)).unwrap_or_default().to_vec(),
        ok: status.get(at(i)).unwrap_or_default().iter().map(is_value).collect(),
        inputs: Inputs { pair, x, y },
    };
    x.iter().zip(y).enumerate().map(row).collect()
}

fn is_value(status: &Status) -> bool {
    matches!(status, Status::Ok | Status::Extrapolated)
}

/// Rows of `outputs` at the points two quantities' values give, in either order: the batch of the pair they make.
pub(crate) fn along(
    batch: &impl Batch,
    outputs: &[Quantity],
    a: (Quantity, &[f64]),
    b: (Quantity, &[f64]),
) -> Result<Vec<Row>, Error> {
    let unpaired = Error::InvalidInput { quantity: "input pair", value: f64::NAN };
    let (pair, swapped) = Pair::from_vars(a.0.var(), b.0.var()).ok_or(unpaired)?;
    let (x, y) = if swapped { (b.1, a.1) } else { (a.1, b.1) };
    batch.rows(outputs, pair, x, y)
}

/// The row's vertex, its first two outputs as x and y; `None` where either failed.
pub(crate) fn vertex(row: &Row) -> Option<Vertex> {
    match (row.cells.first(), row.cells.get(1), row.ok.first(), row.ok.get(1)) {
        (Some(&x), Some(&y), Some(true), Some(true)) => Some(Vertex { x, y, inputs: row.inputs }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each point's cells come from its own stretch of the point-major buffers, and only `Ok` and `Extrapolated` are
    /// values.
    #[test]
    fn rows_split_the_point_major_buffers() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let status = [Status::Ok, Status::Extrapolated, Status::Domain, Status::Ok, Status::Ok, Status::NoConvergence];
        let rows = split(Pair::PT, &[10.0, 20.0, 30.0], &[1.0, 2.0, 3.0], &values, &status, 2);
        let cells: Vec<_> = rows.iter().map(|r| (r.cells.clone(), r.ok.clone())).collect();
        assert_eq!(
            cells,
            [
                (vec![1.0, 2.0], vec![true, true]),
                (vec![3.0, 4.0], vec![false, true]),
                (vec![5.0, 6.0], vec![true, false])
            ]
        );
        assert_eq!(rows[2].inputs, Inputs { pair: Pair::PT, x: 30.0, y: 3.0 });
        assert!(split(Pair::PT, &[1.0], &[1.0], &[], &[], 0)[0].cells.is_empty());
    }

    /// A vertex needs both its x and its y.
    #[test]
    fn a_vertex_needs_its_first_two_cells() {
        let inputs = Inputs { pair: Pair::PT, x: 1.0, y: 2.0 };
        let row = |ok: Vec<bool>| Row { cells: vec![3.0, 4.0, 5.0], ok, inputs };
        assert_eq!(vertex(&row(vec![true, true, false])), Some(Vertex { x: 3.0, y: 4.0, inputs }));
        assert_eq!(vertex(&row(vec![false, true, true])), None);
        assert_eq!(vertex(&row(vec![true, false, true])), None);
        assert_eq!(vertex(&Row { cells: vec![3.0], ok: vec![true], inputs }), None);
    }
}
