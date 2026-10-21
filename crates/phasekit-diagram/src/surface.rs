//! A fluid's thermodynamic surface (PLAN.md M26.1; user decisions DG6, DG8): a triangle mesh over T and ln ρ across
//! the plotted range, whose vertices carry both Gibbs's u–s–v coordinates and the p–v–T ones (v = 1/ρ). The saturated
//! liquid and vapour, the critical point and, where the range starts there, the triple line are edges of the mesh, so
//! that no triangle crosses the dome. It adds no numerics: every vertex is the batch's state at the inputs it carries.

use phasekit_core::{Error, Fluid, Pair, Phase};

use crate::batch::{Batch, FluidBatch, Inputs, Row, along};
use crate::diagram::extent;
use crate::plane::Quantity;
use crate::ranges::{Limits, Ranges, dome_temperatures, lin_grid, log_grid, ranges};

/// A vertex of the surface: the state at its inputs, a `DT` state of the grid or a `QT` state of the dome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceVertex {
    /// Temperature, K.
    pub t: f64,
    /// Density, kg/m³; v = 1/ρ.
    pub rho: f64,
    /// Pressure, Pa.
    pub p: f64,
    /// Specific internal energy, J/kg.
    pub u: f64,
    /// Specific entropy, J/(kg·K).
    pub s: f64,
    /// The vapour quality where the state is two-phase, 0 and 1 on the dome.
    pub q: Option<f64>,
    /// The library's phase at the state.
    pub phase: Phase,
    /// The inputs of the state.
    pub inputs: Inputs,
}

/// The dome on the mesh: indices into its vertices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceDome {
    /// The saturated liquid, Q = 0, in order of temperature up to just below T_c.
    pub liquid: Vec<usize>,
    /// The saturated vapour, Q = 1, at the liquid's temperatures.
    pub vapour: Vec<usize>,
    /// The state at (T_c, ρ_c), where the batch gave it.
    pub critical: Option<usize>,
    /// The triple line, where the plotted range starts at the triple point: its vertices from the saturated vapour to
    /// the saturated liquid, in order of density.
    pub triple: Vec<usize>,
}

/// A fluid's surface: its vertices, its triangles, each counter-clockwise in (ln ρ, T), and its dome.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    /// The vertices, row by row in order of temperature, each row in order of density.
    pub vertices: Vec<SurfaceVertex>,
    /// The triangles, as indices into the vertices.
    pub triangles: Vec<[usize; 3]>,
    /// The dome's edges.
    pub dome: SurfaceDome,
}

/// The fluid's surface, by its own batch under the default options.
pub fn surface(fluid: &Fluid) -> Result<Surface, Error> {
    let batch = FluidBatch::new(fluid);
    sample_surface(&batch, &|inputs| batch.phase(inputs), &Limits::of(fluid)?)
}

/// The surface of the fluid whose batch and limits these are: `phase` gives the library's phase at the state of some
/// inputs, `None` where it refuses the state. One `QT` batch call takes the dome below T_c and one `DT` call the grid:
/// rows at the dome's temperatures, at T_c with the critical point, and above T_c to the plotted range's top, each
/// across the plotted range's densities ([`densities`]), evenly in ln ρ. A state the batch fails, or whose phase
/// the library refuses, is left out and its row joined across the gap; a temperature below T_c without both
/// saturated states is left out, row and all.
pub fn sample_surface(
    batch: &impl Batch,
    phase: &impl Fn(Inputs) -> Option<Phase>,
    limits: &Limits,
) -> Result<Surface, Error> {
    let r = ranges(limits);
    let (t_c, rho_c) = (limits.t_critical, limits.rho_critical);
    let below = if r.t.0 < t_c { dome_temperatures(r.t.0, t_c, DOME_ROWS) } else { Vec::new() };
    let above: Vec<f64> = lin_grid(t_c, r.t.1, ROWS_ABOVE.saturating_add(1)).into_iter().skip(1).collect();
    let (liquid, vapour) = saturated(batch, &below)?;
    let columns = densities(batch, &r, &liquid)?;
    let temperatures: Vec<f64> = below.iter().chain([&t_c]).chain(&above).copied().collect();
    let (critical, grid) = grid(batch, &temperatures, &columns, (rho_c, t_c))?;
    let rows: Vec<&[Row]> = grid.chunks(COLUMNS).collect();
    let (dome_rows, rest) = rows.split_at_checked(below.len()).unwrap_or_default();
    let course = |(&t, rows): (&f64, &&[Row])| Course::plain(t, rows, phase);
    let sides = liquid.iter().zip(&vapour);
    let domed = below.iter().zip(dome_rows).zip(sides).filter_map(|(at, sides)| course(at).with_dome(sides, phase));
    let at_tc = rest.first().map(|rows| course((&t_c, rows)).with_critical(critical.as_ref(), phase));
    let beyond = above.iter().zip(rest.iter().skip(1)).map(course);
    Ok(assemble(domed.chain(at_tc).chain(beyond).collect(), limits.t_triple))
}

/// The batch's outputs at each vertex, its cells 0 to 5.
const OUTPUTS: [Quantity; 6] =
    [Quantity::T, Quantity::Dmass, Quantity::P, Quantity::Umass, Quantity::Smass, Quantity::Q];

/// The densities a row is sampled at, evenly in ln ρ.
const COLUMNS: usize = 48;

/// The rows below T_c, at the dome's temperatures, closing in on T_c.
const DOME_ROWS: usize = 40;

/// The rows above T_c, evenly in T.
const ROWS_ABOVE: usize = 24;

/// The saturated liquid's and vapour's rows at each temperature, by one `QT` call.
fn saturated(batch: &impl Batch, temperatures: &[f64]) -> Result<(Vec<Row>, Vec<Row>), Error> {
    let n = temperatures.len();
    let q: Vec<f64> = [0.0, 1.0].iter().flat_map(|&q| std::iter::repeat_n(q, n)).collect();
    let t = temperatures.repeat(2);
    let mut liquid = batch.rows(&OUTPUTS, Pair::QT, &q, &t)?;
    let vapour = liquid.split_off(n.min(liquid.len()));
    Ok((liquid, vapour))
}

/// `COLUMNS` densities, evenly in ln ρ, from the plotted range's least, at its hot low-pressure corner, to the greatest
/// of its corners and the saturated liquid. The saturated vapour may reach below the least where the triple point's
/// pressure is below the plotted range's (n-Propane's 1.7e-4 Pa gives 1e-8 kg/m³), and its row joins the dome there.
fn densities(batch: &impl Batch, r: &Ranges, liquid: &[Row]) -> Result<Vec<f64>, Error> {
    let (t, p) = ([r.t.0, r.t.0, r.t.1, r.t.1], [r.p.0, r.p.1, r.p.0, r.p.1]);
    let corners = along(batch, &[Quantity::T, Quantity::Dmass], (Quantity::T, &t), (Quantity::P, &p))?;
    let (low, _) = extent(&[&corners], 1);
    let (_, high) = extent(&[&corners, liquid], 1);
    Ok(log_grid(low, high, COLUMNS))
}

/// The rows of the grid's states, row after row, by one `DT` call, the critical point's (at the end of the call) apart.
fn grid(
    batch: &impl Batch,
    temperatures: &[f64],
    columns: &[f64],
    (rho_c, t_c): (f64, f64),
) -> Result<(Option<Row>, Vec<Row>), Error> {
    let at = |&t: &f64| columns.iter().map(move |&rho| (rho, t));
    let points: Vec<(f64, f64)> = temperatures.iter().flat_map(at).chain([(rho_c, t_c)]).collect();
    let (rho, t): (Vec<f64>, Vec<f64>) = points.into_iter().unzip();
    let mut rows = batch.rows(&OUTPUTS, Pair::DT, &rho, &t)?;
    Ok((rows.pop(), rows))
}

/// The vertex a row gives, where its T, ρ, p, u and s are values and the library gives its phase; its Q where that is
/// a value.
fn vertex(row: &Row, phase: &impl Fn(Inputs) -> Option<Phase>) -> Option<SurfaceVertex> {
    let cell = |k: usize| match (row.cells.get(k), row.ok.get(k)) {
        (Some(&v), Some(true)) => Some(v),
        _ => None,
    };
    let (t, rho, p, u, s) = (cell(0)?, cell(1)?, cell(2)?, cell(3)?, cell(4)?);
    Some(SurfaceVertex { t, rho, p, u, s, q: cell(5), phase: phase(row.inputs)?, inputs: row.inputs })
}

/// A row of the mesh at one temperature: its vertices in order of density, and the places in it of the saturated
/// vapour and liquid, or of the critical point twice.
struct Course {
    t: f64,
    vertices: Vec<SurfaceVertex>,
    dome: Option<(usize, usize)>,
}

impl Course {
    /// The grid's vertices at T.
    fn plain(t: f64, rows: &[Row], phase: &impl Fn(Inputs) -> Option<Phase>) -> Course {
        Course { t, vertices: rows.iter().filter_map(|r| vertex(r, phase)).collect(), dome: None }
    }

    /// The row with its saturated liquid and vapour in place; `None` without both.
    fn with_dome(mut self, (liquid, vapour): (&Row, &Row), phase: &impl Fn(Inputs) -> Option<Phase>) -> Option<Course> {
        let (liquid, vapour) = (vertex(liquid, phase)?, vertex(vapour, phase)?);
        let at = self.insert(vapour);
        self.dome = Some((at, self.insert(liquid)));
        Some(self)
    }

    /// The row with the critical point in place, where the batch gave it.
    fn with_critical(mut self, critical: Option<&Row>, phase: &impl Fn(Inputs) -> Option<Phase>) -> Course {
        let at = critical.and_then(|row| vertex(row, phase)).map(|v| self.insert(v));
        self.dome = at.map(|k| (k, k));
        self
    }

    /// Puts the vertex in its place by density, and says where. A grid vertex sampled at its density gives way to it:
    /// the span's ends are the dome's where it reaches them (n-Propane's vapour at its triple point), and the `DT`
    /// state's density there is the input's to within a rounding.
    fn insert(&mut self, v: SurfaceVertex) -> usize {
        self.vertices.retain(|w| !(w.inputs.pair == Pair::DT && w.inputs.x == v.rho));
        let at = self.vertices.partition_point(|w| w.rho < v.rho);
        self.vertices.insert(at, v);
        at
    }
}

/// A course's vertices as indices into the mesh's, and the dome's places in it.
type Chain = (Vec<usize>, Option<(usize, usize)>);

/// The mesh of the courses, in order of temperature: their vertices in turn, the triangles between each two in
/// turn, and the dome's indices.
fn assemble(courses: Vec<Course>, t_triple: f64) -> Surface {
    let mut s = Surface::default();
    let mut chains: Vec<Chain> = Vec::with_capacity(courses.len());
    for c in courses {
        let start = s.vertices.len();
        s.vertices.extend(c.vertices);
        let chain: Vec<usize> = (start..s.vertices.len()).collect();
        let dome = c.dome.and_then(|(v, l)| Some((*chain.get(v)?, *chain.get(l)?)));
        mark(&mut s.dome, dome);
        if c.t == t_triple {
            s.dome.triple = c.dome.and_then(|(v, l)| chain.get(v..=l)).unwrap_or_default().to_vec();
        }
        chains.push((chain, c.dome));
    }
    let pairs = chains.iter().zip(chains.iter().skip(1));
    s.triangles = pairs.flat_map(|(low, high)| strip(low, high, &s.vertices)).collect();
    s
}

/// Records a course's dome: a side each, or the critical point where both are one vertex.
fn mark(dome: &mut SurfaceDome, at: Option<(usize, usize)>) {
    match at {
        Some((v, l)) if v == l => dome.critical = Some(v),
        Some((v, l)) => {
            dome.vapour.push(v);
            dome.liquid.push(l);
        }
        None => {}
    }
}

/// The triangles between two courses: where both meet the dome, a ladder each side of it and one inside, so that the
/// dome's edges are the mesh's; else one ladder across.
fn strip(low: &Chain, high: &Chain, vertices: &[SurfaceVertex]) -> Vec<[usize; 3]> {
    let rho = |i: usize| vertices.get(i).map_or(f64::NAN, |v| v.rho);
    match (low.1, high.1) {
        (Some(a), Some(b)) => {
            let pieces = pieces(&low.0, a).into_iter().zip(pieces(&high.0, b));
            pieces.flat_map(|(l, h)| ladder(l, h, &rho)).collect()
        }
        _ => ladder(&low.0, &high.0, &rho),
    }
}

/// A chain cut at the dome: up to the vapour, from the vapour to the liquid, and from the liquid on.
fn pieces(chain: &[usize], (v, l): (usize, usize)) -> [&[usize]; 3] {
    [chain.get(..=v), chain.get(v..=l), chain.get(l..)].map(Option::unwrap_or_default)
}

/// The triangles between two rows of vertices in order of density, from their first two to their last two: each next
/// vertex, the lower row's first where two tie, closes a triangle with the latest of each row, counter-clockwise in
/// (ln ρ, T).
fn ladder(low: &[usize], high: &[usize], rho: &impl Fn(usize) -> f64) -> Vec<[usize; 3]> {
    let (Some((&a, lows)), Some((&b, highs))) = (low.split_first(), high.split_first()) else {
        return Vec::new();
    };
    let mut steps: Vec<(bool, usize)> =
        lows.iter().map(|&i| (true, i)).chain(highs.iter().map(|&i| (false, i))).collect();
    steps.sort_by(|x, y| rho(x.1).total_cmp(&rho(y.1)));
    let mut ends = (a, b);
    let step = |(lower, next): (bool, usize)| {
        let triangle = [ends.0, next, ends.1];
        if lower {
            ends.0 = next
        } else {
            ends.1 = next
        }
        triangle
    };
    steps.into_iter().map(step).collect()
}
