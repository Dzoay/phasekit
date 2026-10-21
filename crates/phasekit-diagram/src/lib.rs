//! `phasekit-diagram`: the geometry of the thermodynamic diagrams, computed by the library so that every host draws
//! the same lines (PLAN.md M19; user decisions WS4, DG1-DG3). The p–h, T–s, p–T, T–ρ and h–s planes, each with its
//! saturation dome and isolines, as polylines whose every vertex is a state the library computed, carrying the inputs
//! it was computed at; a mixture's traced envelope on p–T and T–ρ; and the phase regions, polygons whose corners carry
//! every plane's axes; and a fluid's thermodynamic surface, a triangle mesh whose vertices carry p, v and T and Gibbs's
//! u, s and v (M26). A line's label is its held quantity and value, a region's its phase. It draws nothing: where a
//! label goes, the colours and the shown units are the host's.
//!
//! It depends on phasekit-core's public API alone (`cargo xtask gates deps`). Its arbiter is the library: a vertex is
//! the batch's value at its inputs, bit for bit.

// PLAN.md M24 (issue #219): no implicit panic, an index out of bounds or an integer overflow, in the library's code
// (`gates lints` checks this line); its tests are exempt. The browser package reaches this crate.
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::arithmetic_side_effects))]

mod batch;
mod diagram;
mod envelope;
mod lines;
mod plane;
mod ranges;
mod refine;
mod regions;
mod surface;

pub use batch::{Batch, FluidBatch, Inputs, Row, Vertex};
pub use diagram::{Diagram, Dome, diagram, sample};
pub use envelope::{Trace, envelope};
pub use lines::{Line, Segment};
pub use plane::{Axis, Family, Plane, Quantity};
pub use ranges::Limits;
pub use regions::{Corner, Region, Regions, regions, sample_regions};
pub use surface::{Surface, SurfaceDome, SurfaceVertex, sample_surface, surface};
