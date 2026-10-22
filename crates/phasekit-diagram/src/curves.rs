//! A fluid's characteristic curves and its compressibility chart (PLAN.md M26.3; user decision DG10). Brown's curves,
//! where Z or one of its derivatives meets the ideal gas's, over the plotted range's lowest temperature to the equation
//! of state's highest: the ideal curve (Z = 1), the Boyle curve ((∂Z/∂ρ)_T = 0), the Joule–Thomson inversion curve
//! ((∂T/∂p)_h = 0) and the Joule inversion curve ((∂Z/∂T)_ρ = 0); and Z against p on isotherms across the plotted
//! range. It adds no numerics: every vertex is the batch's state at the inputs it carries.

use phasekit_core::{Error, Fluid, Pair};

use crate::batch::{Batch, FluidBatch, Inputs, Row, along};
use crate::contour::{Axes, Bracket, Grid, bisect, zero_lines};
use crate::diagram::extent;
use crate::plane::{Axis, Quantity};
use crate::ranges::{Limits, Ranges, log_grid, ranges, values};
use crate::refine::{Halving, Sides, Square, Swept, points, refine};

/// One of Brown's characteristic curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Characteristic {
    /// Z = 1.
    Ideal,
    /// (∂Z/∂ρ)_T = 0.
    Boyle,
    /// (∂T/∂p)_h = 0.
    JouleThomsonInversion,
    /// (∂Z/∂T)_ρ = 0.
    JouleInversion,
}

impl Characteristic {
    /// The four, in Brown's order, from the innermost to the outermost.
    pub const ALL: [Characteristic; 4] = [
        Characteristic::Ideal,
        Characteristic::Boyle,
        Characteristic::JouleThomsonInversion,
        Characteristic::JouleInversion,
    ];

    /// Its condition at a single-phase state, dimensionless and of the derivative's sign: Z − 1; (ρ/p)(∂p/∂ρ)_T − 1,
    /// which is (ρ/Z)(∂Z/∂ρ)_T; (p/T)(∂T/∂p)_h; and (T/p)(∂p/∂T)_ρ − 1, which is (T/Z)(∂Z/∂T)_ρ. `None` where the state
    /// is two-phase (its Q is a value) or a cell it reads failed.
    fn condition(self, row: &Row) -> Option<f64> {
        let cell = |k: usize| cell(row, k);
        if cell(4).is_some() {
            return None;
        }
        let (p, z, t, rho) = (cell(0)?, cell(1)?, cell(2)?, cell(3)?);
        Some(match self {
            Characteristic::Ideal => z - 1.0,
            Characteristic::Boyle => rho * cell(5)? / p - 1.0,
            Characteristic::JouleThomsonInversion => p * cell(6)? / t,
            Characteristic::JouleInversion => t * cell(7)? / p - 1.0,
        })
    }

    /// Its condition at zero density, on the second virial coefficient B and dB/dT at T: B for the ideal and Boyle
    /// curves, T dB/dT − B for the Joule–Thomson inversion curve and dB/dT for the Joule inversion curve.
    const fn virial(self, t: f64, b: f64, db_dt: f64) -> f64 {
        match self {
            Characteristic::Ideal | Characteristic::Boyle => b,
            Characteristic::JouleThomsonInversion => t * db_dt - b,
            Characteristic::JouleInversion => db_dt,
        }
    }
}

/// A vertex of a curve or of an isotherm: the state at its inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveVertex {
    /// Temperature, K.
    pub t: f64,
    /// Density, kg/m³.
    pub rho: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// The compressibility factor p/(ρRT).
    pub z: f64,
    /// The inputs of the state: `DT` on a curve, `PT` or `QT` on an isotherm.
    pub inputs: Inputs,
}

/// A characteristic curve: its lines and where it meets zero density.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// Which curve.
    pub characteristic: Characteristic,
    /// Its lines, each from its lower-density end; a line ends at the sampled range or where the batch has no value
    /// (the two-phase region, or a state it fails).
    pub lines: Vec<Vec<CurveVertex>>,
    /// The temperature at which it meets zero density, where its virial condition has a root in the sampled
    /// temperatures (the lowest, where it has more); `None` where it has none, as the Joule inversion curve's lies
    /// above every equation of state's highest temperature.
    pub zero_density: Option<f64>,
}

/// An isotherm of the compressibility chart: Z against p, in order of pressure, from the saturated vapour to the
/// saturated liquid at the saturation pressure.
#[derive(Clone, Debug, PartialEq)]
pub struct Isotherm {
    /// Its temperature, K.
    pub t: f64,
    /// Its vertices.
    pub vertices: Vec<CurveVertex>,
}

/// A fluid's characteristic curves, in [`Characteristic::ALL`]'s order, and its compressibility chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Curves {
    /// The four curves.
    pub curves: Vec<Curve>,
    /// The chart's isotherms, in order of temperature.
    pub compressibility: Vec<Isotherm>,
}

/// The fluid's curves, by its own batch under the default options.
pub fn curves(fluid: &Fluid) -> Result<Curves, Error> {
    sample_curves(&FluidBatch::new(fluid), &Limits::of(fluid)?)
}

/// The curves of the fluid whose batch and limits these are. One `DT` call takes a grid of `ROWS` temperatures from
/// the plotted range's lowest to the equation of state's highest and `COLUMNS` densities across them (`densities`),
/// each evenly in its logarithm; a curve's vertices are its condition's zeros on the grid's edges, by bisection.
pub fn sample_curves(batch: &impl Batch, limits: &Limits) -> Result<Curves, Error> {
    let r = ranges(limits);
    let t = log_grid(r.t.0, limits.t_max, ROWS);
    let grid = Grid::sample(batch, &OUTPUTS, Axes::DT, (densities(batch, &r, limits.t_max)?, t))?;
    let low = grid.x.first().copied().unwrap_or(f64::NAN);
    let curve = |characteristic: Characteristic| -> Result<Curve, Error> {
        let lines = zero_lines(batch, &OUTPUTS, &grid, &|row| characteristic.condition(row))?;
        let lines = lines.iter().map(|rows| rows.iter().filter_map(vertex).collect()).collect();
        let zero_density = zero_density(batch, characteristic, low, &grid.y)?;
        Ok(Curve { characteristic, lines, zero_density })
    };
    let curves = Characteristic::ALL.into_iter().map(curve).collect::<Result<_, _>>()?;
    Ok(Curves { curves, compressibility: compressibility(batch, &r)? })
}

/// The batch's outputs at each vertex: p, Z, T, ρ and Q, then the curves' derivatives.
const OUTPUTS: [Quantity; 8] = [
    Quantity::P,
    Quantity::Z,
    Quantity::T,
    Quantity::Dmass,
    Quantity::Q,
    Quantity::DpDrhoAtT,
    Quantity::JouleThomson,
    Quantity::DpDTAtRho,
];

/// The chart's outputs: `OUTPUTS`' first five.
const CHART: [Quantity; 5] = [Quantity::P, Quantity::Z, Quantity::T, Quantity::Dmass, Quantity::Q];

/// The grid's temperatures.
const ROWS: usize = 64;

/// The grid's densities.
const COLUMNS: usize = 64;

/// The points an isotherm of the chart starts on, evenly in ln p across the plotted range.
const SWEEP_POINTS: usize = 32;

/// The cell's value, where it is one.
fn cell(row: &Row, k: usize) -> Option<f64> {
    match (row.cells.get(k), row.ok.get(k)) {
        (Some(&v), Some(true)) => Some(v),
        _ => None,
    }
}

/// The vertex a row of `OUTPUTS`' first four gives, where they are values.
fn vertex(row: &Row) -> Option<CurveVertex> {
    let c = |k| cell(row, k);
    Some(CurveVertex { p: c(0)?, z: c(1)?, t: c(2)?, rho: c(3)?, inputs: row.inputs })
}

/// `COLUMNS` densities, evenly in ln ρ, from the least of the corners of the plotted pressures at the plotted range's
/// lowest temperature and at `t_max` (at `t_max` and the lowest pressure) to the greatest of them and the saturated
/// liquid's at the lowest temperature.
fn densities(batch: &impl Batch, r: &Ranges, t_max: f64) -> Result<Vec<f64>, Error> {
    let (t, p) = ([r.t.0, r.t.0, t_max, t_max], [r.p.0, r.p.1, r.p.0, r.p.1]);
    let corners = along(batch, &[Quantity::Dmass], (Quantity::T, &t), (Quantity::P, &p))?;
    let liquid = along(batch, &[Quantity::Dmass], (Quantity::T, &[r.t.0]), (Quantity::Q, &[0.0]))?;
    let (low, high) = extent(&[&corners, &liquid], 0);
    Ok(log_grid(low, high, COLUMNS))
}

/// The batch's outputs on the zero-density line: T, B and dB/dT.
const VIRIALS: [Quantity; 3] = [Quantity::T, Quantity::Bvirial, Quantity::DBvirialDT];

/// The lowest temperature of `temperatures`' span at which the curve's virial condition is zero, by bisection between
/// the first two neighbours on either side of it, at the density `rho`; `None` where no two are.
fn zero_density(batch: &impl Batch, c: Characteristic, rho: f64, temperatures: &[f64]) -> Result<Option<f64>, Error> {
    let field = |row: &Row| Some(c.virial(cell(row, 0)?, cell(row, 1)?, cell(row, 2)?));
    let rows = batch.rows(&VIRIALS, Pair::DT, &vec![rho; temperatures.len()], temperatures)?;
    let valued: Vec<(f64, &Row)> = rows.iter().filter_map(|row| Some((field(row)?, row))).collect();
    let first = valued.windows(2).find_map(|w| match w {
        [(a, ra), (b, rb)] if (*a >= 0.0) != (*b >= 0.0) => {
            Some(Bracket::new((*a, (*ra).clone()), (*b, (*rb).clone())))
        }
        _ => None,
    });
    let found = bisect(batch, (&VIRIALS, Axes::DT), &field, vec![first], 0.0)?;
    Ok(found.into_iter().flatten().next().and_then(|row| cell(&row, 0)))
}

/// The chart's isotherms: at the plotted temperatures' nice values, each across the plotted pressures, refined where
/// it bends in (ln p, Z) and, below the critical temperature, split at its saturation pressure.
fn compressibility(batch: &impl Batch, r: &Ranges) -> Result<Vec<Isotherm>, Error> {
    let temperatures = values(Quantity::T, r.t);
    let p = log_grid(r.p.0, r.p.1, SWEEP_POINTS);
    let n = temperatures.len();
    let all_t: Vec<f64> = temperatures.iter().flat_map(|&t| std::iter::repeat_n(t, p.len())).collect();
    let starts = along(batch, &CHART, (Quantity::P, &p.repeat(n)), (Quantity::T, &all_t))?;
    let q: Vec<f64> = [0.0, 1.0].iter().flat_map(|&q| std::iter::repeat_n(q, n)).collect();
    let mut liquid = along(batch, &CHART, (Quantity::T, &temperatures.repeat(2)), (Quantity::Q, &q))?;
    let vapour = liquid.split_off(n.min(liquid.len()));
    let square = Square::new((Axis { quantity: Quantity::P, log: true }, r.p), (z_axis(), extent(&[&starts], 1)));
    let chart = Chart { batch, square: &square };
    let each = temperatures.iter().zip(starts.chunks(p.len())).zip(liquid.iter().zip(&vapour));
    each.map(|((&t, rows), sides)| chart.isotherm(t, &p, rows, sides)).collect()
}

const fn z_axis() -> Axis {
    Axis { quantity: Quantity::Z, log: false }
}

/// What an isotherm of the chart is refined in.
struct Chart<'a, B> {
    batch: &'a B,
    square: &'a Square,
}

impl<B: Batch> Chart<'_, B> {
    /// The isotherm at `t` from its rows at the pressures `p` and its saturated liquid and vapour: one piece where
    /// either saturated state failed (above the critical temperature), else the vapour's pressures below the
    /// saturated vapour's and the liquid's above the saturated liquid's.
    fn isotherm(&self, t: f64, p: &[f64], rows: &[Row], (liquid, vapour): (&Row, &Row)) -> Result<Isotherm, Error> {
        let single: Vec<Swept> = points(p, rows.to_vec()).into_iter().filter(|s| cell(&s.row, 4).is_none()).collect();
        let pieces = match (cell(liquid, 0), cell(vapour, 0)) {
            (Some(bubble), Some(dew)) => {
                let edge = |row: &Row, at: f64| Swept { at, row: row.clone(), edge: true };
                let (gas, rest): (Vec<Swept>, Vec<Swept>) = single.into_iter().partition(|s| s.at < dew);
                let dense = rest.into_iter().filter(|s| s.at > bubble);
                vec![
                    gas.into_iter().chain([edge(vapour, dew)]).collect(),
                    [edge(liquid, bubble)].into_iter().chain(dense).collect(),
                ]
            }
            _ => vec![single],
        };
        let mut vertices = Vec::new();
        for piece in pieces {
            vertices.extend(self.refined(t, piece)?.iter().filter_map(|s| vertex(&s.row)));
        }
        Ok(Isotherm { t, vertices })
    }

    fn refined(&self, t: f64, piece: Vec<Swept>) -> Result<Vec<Swept>, Error> {
        let eval = |p: &[f64]| along(self.batch, &CHART, (Quantity::P, p), (Quantity::T, &vec![t; p.len()]));
        refine(piece, (Halving::Geometric, Sides::One), self.square, eval)
    }
}
