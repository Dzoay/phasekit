//! A three-component mixture's ternary diagram at fixed T and p (PLAN.md M26.5): on the composition triangle, the
//! two-phase region's boundary, the binodal, as lines; its tie lines, each the two phases of a feed inside it; and the
//! region itself, each lattice triangle's piece of it as a polygon, as M26.4's bands are (user decision DG11), so that
//! the host fills it and computes no geometry. The feeds lie on a lattice of [`DIVISIONS`] steps a side, the corners
//! and the binary sides included, each classified in one call by the caller's equilibrium: the tangent-plane stability
//! test, and the split where it finds the feed unstable. A lattice edge whose ends' verdicts differ is halved
//! [`BISECTIONS`] times, M19.3's depth, one call a level, and the binodal crosses it at its bracket's split end, within
//! [`RESOLUTION`] of a feed the test finds stable; marching triangles join the crossings into lines. A feed whose
//! equilibrium fails leaves its triangles out, and a line ends there. The equilibria reach the crate through
//! [`TernaryBatch`], which the browser package fills from `phasekit-mix`, so that the crate depends on core alone (user
//! decision DG3's rule). Every point drawn is a feed the caller's equilibrium classified.

use std::collections::BTreeMap;

use phasekit_core::Error;

use crate::contour::chains;

/// The lattice's steps along each side of the triangle: 325 feeds, 576 triangles.
const DIVISIONS: u32 = 24;

/// The times a lattice edge across the binodal is halved, M19.3's depth.
const BISECTIONS: u32 = 6;

/// How far a binodal vertex lies from a feed the stability test finds stable, in each mole fraction, to rounding: a
/// lattice step, 1/24, halved 6 times.
pub const RESOLUTION: f64 = 1.0 / 1536.0;

/// The lattice's every third feed along each of its first two components is a tie line's where it splits.
const TIE_STEP: u32 = 3;

/// A feed's two phases at the diagram's T and p.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    /// The vapour's molar fraction of the feed, in (0, 1).
    pub beta: f64,
    /// The liquid's mole fractions.
    pub liquid: [f64; 3],
    /// The vapour's mole fractions.
    pub vapour: [f64; 3],
}

/// A feed's phases at the diagram's T and p, as the stability test and the split give them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phases {
    /// One phase, stable.
    Single,
    /// Two phases at equal fugacities.
    Split(Split),
}

impl Phases {
    fn split(&self) -> Option<Split> {
        match self {
            Phases::Split(split) => Some(*split),
            Phases::Single => None,
        }
    }
}

/// A three-component mixture at one T and p, as the diagram samples it.
pub trait TernaryBatch {
    /// The temperature, K.
    fn t(&self) -> f64;

    /// The pressure, Pa.
    fn p(&self) -> f64;

    /// The phases of each feed, its three mole fractions summing to 1, any of them 0 (a binary mixture, or a pure
    /// component); `None` where its equilibrium fails.
    fn phases(&self, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error>;
}

/// A tie line: a feed inside the two-phase region and its two phases, the line's ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TieLine {
    /// The feed's mole fractions.
    pub feed: [f64; 3],
    /// Its phases.
    pub split: Split,
}

/// A point of the binodal: the feed inside the region at the end of its bracket, its phases, and the feed outside at
/// the other end, which the stability test finds stable, within [`RESOLUTION`] of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinodalVertex {
    /// The feed inside, which splits.
    pub feed: [f64; 3],
    /// Its phases.
    pub split: Split,
    /// The feed outside, one phase.
    pub outside: [f64; 3],
}

/// A three-component mixture's ternary diagram at one T and p, in mole fractions.
#[derive(Clone, Debug, PartialEq)]
pub struct Ternary {
    /// The temperature, K.
    pub t: f64,
    /// The pressure, Pa.
    pub p: f64,
    /// The binodal's lines, each in order along it, a closed one returning to its first vertex; a line ends on the
    /// triangle's side or where a feed's equilibrium failed.
    pub binodal: Vec<Vec<BinodalVertex>>,
    /// The tie lines, at the lattice's every third feed along each of its first two components that splits.
    pub tie_lines: Vec<TieLine>,
    /// The two-phase region, each lattice triangle's piece of it: the triangle's corners that split and the binodal's
    /// crossings between, counter-clockwise in (z₀, z₁).
    pub region: Vec<Vec<[f64; 3]>>,
}

/// A node of the lattice, (i, j): the feed (i, j, 24 − i − j) / 24.
type Node = (u32, u32);

/// A lattice edge, its ends in order.
type Edge = (Node, Node);

/// Each node's phases, where its equilibrium gave them.
type Verdicts = BTreeMap<Node, Phases>;

/// The node's feed.
fn feed((i, j): Node) -> [f64; 3] {
    let k = DIVISIONS.saturating_sub(i).saturating_sub(j);
    [i, j, k].map(|c| f64::from(c) / f64::from(DIVISIONS))
}

/// Every node of the lattice.
fn nodes() -> impl Iterator<Item = Node> {
    (0..=DIVISIONS).flat_map(|i| (0..=DIVISIONS.saturating_sub(i)).map(move |j| (i, j)))
}

/// The lattice's triangles, each its corners counter-clockwise in (i, j): (i, j), (i + 1, j), (i, j + 1) and, where
/// the lattice has room, (i + 1, j), (i + 1, j + 1), (i, j + 1).
fn triangles() -> Vec<[Node; 3]> {
    let mut all = Vec::new();
    for (i, j) in nodes() {
        let (i1, j1) = (i.saturating_add(1), j.saturating_add(1));
        let room = DIVISIONS.saturating_sub(i).saturating_sub(j);
        if room >= 1 {
            all.push([(i, j), (i1, j), (i, j1)]);
        }
        if room >= 2 {
            all.push([(i1, j), (i1, j1), (i, j1)]);
        }
    }
    all
}

/// A triangle's sides in order around it, each from a corner to the next.
fn sides([a, b, c]: [Node; 3]) -> [(Node, Node); 3] {
    [(a, b), (b, c), (c, a)]
}

fn edge(a: Node, b: Node) -> Edge {
    (a.min(b), a.max(b))
}

/// Whether the node's feed splits, where its equilibrium gave its phases.
fn splits(verdicts: &Verdicts, node: Node) -> Option<bool> {
    verdicts.get(&node).map(|phases| phases.split().is_some())
}

/// The phases of each feed, one call, as many as there are feeds.
fn classify(batch: &dyn TernaryBatch, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error> {
    let phases = batch.phases(feeds)?;
    if phases.len() != feeds.len() {
        return Err(Error::Shape { expected: feeds.len(), found: phases.len() });
    }
    Ok(phases)
}

/// The bracket of the edge from `a` to `b` where one end splits and the other is one phase.
fn bracket(verdicts: &Verdicts, a: Node, b: Node) -> Option<BinodalVertex> {
    let at = |inside: Node, split: &Split, outside: Node| BinodalVertex {
        feed: feed(inside),
        split: *split,
        outside: feed(outside),
    };
    match (verdicts.get(&a)?, verdicts.get(&b)?) {
        (Phases::Split(split), Phases::Single) => Some(at(a, split, b)),
        (Phases::Single, Phases::Split(split)) => Some(at(b, split, a)),
        _ => None,
    }
}

impl BinodalVertex {
    /// The midpoint of the bracket's ends.
    fn mid(&self) -> [f64; 3] {
        let mut mid = [0.0; 3];
        for ((m, a), b) in mid.iter_mut().zip(self.feed).zip(self.outside) {
            *m = (a + b) / 2.0;
        }
        mid
    }

    /// The half of the bracket on which the verdict changes, from the phases at its midpoint `at`; `None` where they
    /// failed.
    fn halve(self, at: [f64; 3], phases: Option<Phases>) -> Option<BinodalVertex> {
        match phases? {
            Phases::Split(split) => Some(BinodalVertex { feed: at, split, ..self }),
            Phases::Single => Some(BinodalVertex { outside: at, ..self }),
        }
    }
}

/// Each bracket halved `BISECTIONS` times, one call a level; one whose midpoint's equilibrium failed is left out.
fn bisect(
    batch: &dyn TernaryBatch,
    brackets: BTreeMap<Edge, BinodalVertex>,
) -> Result<BTreeMap<Edge, BinodalVertex>, Error> {
    let mut open = brackets;
    for _ in 0..BISECTIONS {
        let mids: Vec<[f64; 3]> = open.values().map(BinodalVertex::mid).collect();
        let phases = classify(batch, &mids)?;
        let halves = mids.into_iter().zip(phases);
        open = open.into_iter().zip(halves).filter_map(|((e, b), (at, p))| Some((e, b.halve(at, p)?))).collect();
    }
    Ok(open)
}

/// The triangle's segment of the binodal: the two sides it crosses, where every corner has its phases and both
/// crossings were found.
fn segment(verdicts: &Verdicts, crossings: &BTreeMap<Edge, BinodalVertex>, corners: [Node; 3]) -> Option<(Edge, Edge)> {
    let mut crossed = Vec::new();
    for (a, b) in sides(corners) {
        if splits(verdicts, a)? != splits(verdicts, b)? {
            crossed.push(edge(a, b));
        }
    }
    match crossed[..] {
        [e, f] if crossings.contains_key(&e) && crossings.contains_key(&f) => Some((e, f)),
        _ => None,
    }
}

/// The triangle's piece of the two-phase region: its corners that split and the crossings on the sides between, in
/// order around it; none where a corner lacks its phases, a crossing was not found, or no corner splits.
fn piece(verdicts: &Verdicts, crossings: &BTreeMap<Edge, BinodalVertex>, corners: [Node; 3]) -> Option<Vec<[f64; 3]>> {
    let mut polygon = Vec::new();
    for (a, b) in sides(corners) {
        let (from, to) = (splits(verdicts, a)?, splits(verdicts, b)?);
        if from {
            polygon.push(feed(a));
        }
        if from != to {
            polygon.push(crossings.get(&edge(a, b))?.feed);
        }
    }
    (!polygon.is_empty()).then_some(polygon)
}

/// The tie lines: each feed at every `TIE_STEP`-th node along i and j that splits.
fn tie_lines(verdicts: &Verdicts) -> Vec<TieLine> {
    let on = |(i, j): Node| i.is_multiple_of(TIE_STEP) && j.is_multiple_of(TIE_STEP);
    let tie = |(&node, phases): (&Node, &Phases)| Some(TieLine { feed: feed(node), split: phases.split()? });
    verdicts.iter().filter(|&(&node, _)| on(node)).filter_map(tie).collect()
}

/// The ternary diagram of `batch` at its T and p (see the module's notes).
pub fn ternary(batch: &dyn TernaryBatch) -> Result<Ternary, Error> {
    let nodes: Vec<Node> = nodes().collect();
    let feeds: Vec<[f64; 3]> = nodes.iter().map(|&n| feed(n)).collect();
    let phases = classify(batch, &feeds)?;
    let verdicts: Verdicts = nodes.into_iter().zip(phases).filter_map(|(n, p)| Some((n, p?))).collect();
    let triangles = triangles();
    let edges = triangles.iter().flat_map(|&t| sides(t));
    let brackets = edges.filter_map(|(a, b)| Some((edge(a, b), bracket(&verdicts, a, b)?))).collect();
    let crossings = bisect(batch, brackets)?;
    let segments: Vec<(Edge, Edge)> = triangles.iter().filter_map(|&t| segment(&verdicts, &crossings, t)).collect();
    let vertices = |chain: Vec<Edge>| chain.iter().filter_map(|e| crossings.get(e).copied()).collect();
    let binodal = chains(&segments).into_iter().map(vertices).collect();
    let region = triangles.iter().filter_map(|&t| piece(&verdicts, &crossings, t)).collect();
    Ok(Ternary { t: batch.t(), p: batch.p(), binodal, tie_lines: tie_lines(&verdicts), region })
}

#[cfg(test)]
#[allow(clippy::disallowed_types, reason = "a test-only record of the batch's calls; the library keeps none (D8)")]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The band's bounds in z₀: a feed between them splits.
    const LO: f64 = 0.3;
    const HI: f64 = 0.6;

    /// A mixture whose feeds split where LO ≤ z₀ ≤ HI, into a liquid at z₀ = LO and a vapour at z₀ = HI, the other two
    /// components in the feed's ratio, and whose equilibrium fails where `fails` holds of the feed.
    struct Band {
        fails: fn([f64; 3]) -> bool,
        calls: RefCell<Vec<usize>>,
    }

    fn never(_: [f64; 3]) -> bool {
        false
    }

    impl Band {
        fn new(fails: fn([f64; 3]) -> bool) -> Band {
            Band { fails, calls: RefCell::new(Vec::new()) }
        }

        fn at(z: [f64; 3]) -> Phases {
            if !(LO..=HI).contains(&z[0]) {
                return Phases::Single;
            }
            let at = |z0: f64| [z0, z[1] * (1.0 - z0) / (1.0 - z[0]), z[2] * (1.0 - z0) / (1.0 - z[0])];
            Phases::Split(Split { beta: (z[0] - LO) / (HI - LO), liquid: at(LO), vapour: at(HI) })
        }
    }

    impl TernaryBatch for Band {
        fn t(&self) -> f64 {
            273.15
        }

        fn p(&self) -> f64 {
            5e5
        }

        fn phases(&self, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error> {
            self.calls.borrow_mut().push(feeds.len());
            Ok(feeds.iter().map(|&z| (!(self.fails)(z)).then(|| Band::at(z))).collect())
        }
    }

    /// The area of a polygon of compositions in (z₀, z₁), positive where it runs counter-clockwise.
    fn area(polygon: &[[f64; 3]]) -> f64 {
        let next = polygon.iter().cycle().skip(1);
        polygon.iter().zip(next).map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum::<f64>() / 2.0
    }

    /// The lattice: 325 nodes, from (0, 0, 1) to (1, 0, 0), and 576 triangles, each counter-clockwise, half of the
    /// triangle's area of 1/2 over 24², every corner a node.
    #[test]
    fn the_lattice_is_576_triangles_counter_clockwise() {
        assert_eq!(nodes().count(), 325);
        assert_eq!([feed((0, 0)), feed((24, 0)), feed((6, 12))], [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.25, 0.5, 0.25]]);
        let all = triangles();
        assert_eq!(all.len(), 576);
        let nodes: Vec<Node> = nodes().collect();
        for t in &all {
            assert!(t.iter().all(|n| nodes.contains(n)), "{t:?}");
            let a = area(&t.map(feed));
            assert!((a - 0.5 / 576.0).abs() < 1e-15, "{t:?}: {a}");
        }
        assert_eq!((0..BISECTIONS).fold(1.0 / f64::from(DIVISIONS), |r, _| r / 2.0), RESOLUTION);
    }

    /// The band's binodal: its two lines, each vertex inside the band within `RESOLUTION` of its outside feed, which
    /// lies outside it; the lattice in one call, then one call a level.
    #[test]
    fn the_binodal_brackets_the_verdicts_change_within_the_resolution() {
        let band = Band::new(never);
        let d = ternary(&band).unwrap();
        assert_eq!((d.t, d.p), (273.15, 5e5));
        let calls = band.calls.borrow();
        assert_eq!((calls.len(), calls.first()), (7, Some(&325)));
        assert_eq!(d.binodal.len(), 2);
        for v in d.binodal.iter().flatten() {
            assert!((LO..=HI).contains(&v.feed[0]) && !(LO..=HI).contains(&v.outside[0]), "{v:?}");
            let apart = (0..3).map(|i| (v.feed[i] - v.outside[i]).abs()).fold(0.0, f64::max);
            assert!(apart > 0.0 && apart <= RESOLUTION * (1.0 + 1e-9), "{v:?}: {apart}");
            assert_eq!(Phases::Split(v.split), Band::at(v.feed));
        }
    }

    /// Each of the band's lines runs from the side z₁ = 0 to the side z₂ = 0, its vertices along it.
    #[test]
    fn each_line_runs_from_side_to_side() {
        let d = ternary(&Band::new(never)).unwrap();
        for line in &d.binodal {
            let (first, last) = (line.first().unwrap().feed, line.last().unwrap().feed);
            let mut ends = [first[1], first[2], last[1], last[2]].map(|v| v == 0.0);
            ends.sort();
            assert_eq!(ends, [false, false, true, true], "{first:?} {last:?}");
            for pair in line.windows(2) {
                let step = (0..3).map(|i| (pair[0].feed[i] - pair[1].feed[i]).abs()).fold(0.0, f64::max);
                assert!(step <= 1.0 / 24.0 + RESOLUTION, "{pair:?}");
            }
        }
    }

    /// The band's region: pieces whose every corner is inside the band, each counter-clockwise, adding up to its area,
    /// ∫ (1 − z₀) dz₀ from LO to HI, within the binodal's resolution along its 1.1 of length.
    #[test]
    fn the_region_is_the_bands_area() {
        let d = ternary(&Band::new(never)).unwrap();
        let mut total = 0.0;
        for piece in &d.region {
            assert!(piece.len() >= 3 && piece.iter().all(|z| (LO..=HI).contains(&z[0])), "{piece:?}");
            let a = area(piece);
            assert!(a > 0.0, "{piece:?}");
            total += a;
        }
        let want = (HI - LO) - (HI * HI - LO * LO) / 2.0;
        assert!((total - want).abs() < 1.1 * RESOLUTION, "{total} {want}");
    }

    /// The tie lines: the band's feeds at every third node along i and j, i of 9 and 12, each its split.
    #[test]
    fn the_tie_lines_are_every_third_feed_that_splits() {
        let d = ternary(&Band::new(never)).unwrap();
        let want: Vec<[f64; 3]> =
            [9, 12].into_iter().flat_map(|i| (0..=24 - i).step_by(3).map(move |j| feed((i, j)))).collect();
        let got: Vec<[f64; 3]> = d.tie_lines.iter().map(|l| l.feed).collect();
        assert_eq!(got.len(), want.len());
        assert!(want.iter().all(|z| got.contains(z)), "{got:?}");
        assert!(d.tie_lines.iter().all(|l| Phases::Split(l.split) == Band::at(l.feed)));
    }

    /// Feeds whose equilibrium fails, where 0.45 < z₁ < 0.55, split the line at z₀ = LO, which crosses them, and leave
    /// out their triangles' pieces; the line at HI, which runs to z₁ = 0.4, stays whole.
    #[test]
    fn a_failed_feed_ends_its_lines_and_leaves_its_triangles_out() {
        let gap = |z: [f64; 3]| z[1] > 0.45 && z[1] < 0.55;
        let d = ternary(&Band::new(gap)).unwrap();
        let near = |line: &Vec<BinodalVertex>, at: f64| line.iter().all(|v| (v.feed[0] - at).abs() <= RESOLUTION);
        let lo = d.binodal.iter().filter(|l| near(l, LO)).count();
        let hi = d.binodal.iter().filter(|l| near(l, HI)).count();
        assert_eq!((d.binodal.len(), lo, hi), (3, 2, 1));
        let cut = |z: &[f64; 3]| z[1] > 11.0 / 24.0 - 1e-12 && z[1] < 13.0 / 24.0 + 1e-12;
        assert!(d.region.iter().flatten().all(|z| !cut(z)));
        let whole = ternary(&Band::new(never)).unwrap();
        assert!(d.region.len() < whole.region.len());
    }

    /// A batch whose midpoint fails leaves that crossing out, and its triangles' segments and pieces with it.
    #[test]
    fn a_crossing_whose_midpoint_fails_is_left_out() {
        let odd = |z: [f64; 3]| (z[0] * 24.0).fract() != 0.0 && z[1] < 0.2;
        let d = ternary(&Band::new(odd)).unwrap();
        let whole = ternary(&Band::new(never)).unwrap();
        assert!(d.binodal.iter().flatten().all(|v| v.feed[1] >= 0.2 - 1e-12));
        assert!(d.region.len() < whole.region.len() && !d.region.is_empty());
    }

    /// One crossing whose midpoint fails, on the edge along z₁ = 1/2 across z₀ = LO, splits that line in two there,
    /// and leaves out the pieces of the two triangles that share the edge; the line at HI stays whole.
    #[test]
    fn a_single_missing_crossing_splits_its_line() {
        let one = |z: [f64; 3]| (z[0] * 24.0).fract() != 0.0 && z[1] == 0.5;
        let d = ternary(&Band::new(one)).unwrap();
        let whole = ternary(&Band::new(never)).unwrap();
        assert_eq!((d.binodal.len(), whole.binodal.len()), (3, 2));
        assert!(d.binodal.iter().all(|line| line.len() >= 2));
        assert_eq!(d.region.len() + 2, whole.region.len());
    }

    /// One phase everywhere: no binodal, tie line or region. Two everywhere: every triangle whole, every third feed
    /// a tie line's, no binodal.
    #[test]
    fn one_phase_everywhere_has_no_region_and_two_everywhere_all_of_it() {
        struct Everywhere(Phases);
        impl TernaryBatch for Everywhere {
            fn t(&self) -> f64 {
                300.0
            }
            fn p(&self) -> f64 {
                1e5
            }
            fn phases(&self, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error> {
                Ok(vec![Some(self.0); feeds.len()])
            }
        }
        let one = ternary(&Everywhere(Phases::Single)).unwrap();
        assert!(one.binodal.is_empty() && one.tie_lines.is_empty() && one.region.is_empty());
        let split = Split { beta: 0.5, liquid: [0.2, 0.3, 0.5], vapour: [0.5, 0.3, 0.2] };
        let two = ternary(&Everywhere(Phases::Split(split))).unwrap();
        assert!(two.binodal.is_empty());
        assert_eq!((two.region.len(), two.tie_lines.len()), (576, 45));
        let triangles: Vec<Vec<[f64; 3]>> = triangles().into_iter().map(|t| t.map(feed).to_vec()).collect();
        assert_eq!(two.region, triangles);
    }

    /// A batch that answers for a different number of feeds is a `Shape` error, and its own error stands.
    #[test]
    fn a_batch_that_answers_wrongly_is_refused() {
        struct Short(bool);
        impl TernaryBatch for Short {
            fn t(&self) -> f64 {
                300.0
            }
            fn p(&self) -> f64 {
                1e5
            }
            fn phases(&self, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error> {
                if self.0 {
                    return Err(Error::InvalidArgument { argument: "feed", value: None });
                }
                Ok(vec![Some(Phases::Single); feeds.len().saturating_sub(1)])
            }
        }
        assert_eq!(ternary(&Short(false)), Err(Error::Shape { expected: 325, found: 324 }));
        let refused = Error::InvalidArgument { argument: "feed", value: None };
        assert_eq!(ternary(&Short(true)), Err(refused));
    }
}
