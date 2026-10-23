//! A ternary diagram (PLAN.md M26.5) of R407C's components, R32, R125 and R134a, at 273.15 K and 5 bar, where R407C
//! splits, its equilibria `phasekit-mix`'s PT flash with the stability test (a dev-dependency only; the crate takes
//! them through `TernaryBatch`, user decision DG3's rule): every tie line is the flash of its feed (`Exact`), the feed
//! on it by the lever rule within `Flash`; the binodal lies where the stability test's verdict changes, within the
//! refinement's resolution; R407C itself lies inside the region at 5 bar and outside it at 4 bar, as its flash says.

use std::sync::LazyLock;

use phasekit_core::{Basis, DataSet, Error, Input, Pair, Registry};
use phasekit_diagram::{Phases, RESOLUTION, Split, Ternary, TernaryBatch, ternary};
use phasekit_mix::{Equilibrium, Mixture, MultiFluid};
use phasekit_verify::tolerance::ToleranceClass;

/// R407C's components, in its order.
const NAMES: [&str; 3] = ["R32", "R125", "R134a"];

/// R407C's mole fractions (CoolProp's R407C.mix, its mass fractions 23/25/52 %).
const R407C: [f64; 3] = [0.381109419953993, 0.179558888662016, 0.439331691383991];

/// The diagram's temperature, K, and pressures, Pa: R407C splits at the first and is one phase at the second.
const T: f64 = 273.15;
const SPLITS: f64 = 5e5;
const SINGLE: f64 = 4e5;

/// R407C's components at (T, p), each feed by phasekit-mix: its non-zero components' PT equilibrium, or a pure
/// component's PT state, one phase.
struct Components {
    t: f64,
    p: f64,
}

impl Components {
    fn equilibrium(&self, z: [f64; 3]) -> Result<Phases, Error> {
        let present: Vec<usize> = (0..3).filter(|&i| z[i] > 0.0).collect();
        let registry = Registry::embedded()?;
        if let [one] = present[..] {
            let fluid = registry.get(NAMES[one])?;
            fluid.state(Input::new(Pair::PT, self.p, self.t, Basis::Mass)?)?;
            return Ok(Phases::Single);
        }
        let names: Vec<&str> = present.iter().map(|&i| NAMES[i]).collect();
        let fractions: Vec<f64> = present.iter().map(|&i| z[i]).collect();
        let mixture = Mixture::new(MultiFluid::new(registry, &names, DataSet::Corrected)?, &fractions)?;
        let whole = |part: &[f64]| {
            let mut out = [0.0; 3];
            for (&i, &x) in present.iter().zip(part) {
                out[i] = x;
            }
            out
        };
        match mixture.equilibrium(self.t, self.p)? {
            Equilibrium::Single { .. } => Ok(Phases::Single),
            Equilibrium::Split { beta, x, y, .. } => {
                Ok(Phases::Split(Split { beta, liquid: whole(&x), vapour: whole(&y) }))
            }
            _ => Err(Error::InvalidArgument { argument: "equilibrium", value: None }),
        }
    }
}

impl TernaryBatch for Components {
    fn t(&self) -> f64 {
        self.t
    }

    fn p(&self) -> f64 {
        self.p
    }

    fn phases(&self, feeds: &[[f64; 3]]) -> Result<Vec<Option<Phases>>, Error> {
        Ok(feeds.iter().map(|&z| self.equilibrium(z).ok()).collect())
    }
}

/// The diagram at 5 bar, where R407C splits.
static AT_SPLIT: LazyLock<Result<Ternary, Error>> = LazyLock::new(|| ternary(&Components { t: T, p: SPLITS }));

fn diagram() -> Result<&'static Ternary, Error> {
    AT_SPLIT.as_ref().map_err(Clone::clone)
}

/// Each tie line is the flash of its feed bit for bit, and the feed lies on it by the lever rule within `Flash`:
/// (1 − β) x + β y = z. So is each binodal vertex's split.
#[test]
fn each_tie_line_is_the_flash_of_a_feed_on_it() {
    let d = diagram().unwrap();
    let at = Components { t: T, p: SPLITS };
    assert!(d.tie_lines.len() >= 5, "{} tie lines", d.tie_lines.len());
    assert_eq!((d.t, d.p), (T, SPLITS));
    let vertices = d.binodal.iter().flatten().map(|v| (v.feed, v.split));
    for (feed, split) in d.tie_lines.iter().map(|l| (l.feed, l.split)).chain(vertices) {
        assert_eq!(at.equilibrium(feed), Ok(Phases::Split(split)), "{feed:?}");
        let bound = ToleranceClass::Flash.bound(1.0).unwrap();
        for i in 0..3 {
            let lever = (1.0 - split.beta) * split.liquid[i] + split.beta * split.vapour[i];
            assert!((lever - feed[i]).abs() <= bound, "{feed:?} {i}: {lever}");
        }
    }
}

/// Each binodal vertex is a feed the stability test finds unstable, within the refinement's resolution of one it finds
/// stable, to rounding; the binodal's two lines, the bubble and the dew side, each run from the R32–R134a edge to the R125–R134a
/// edge, since R32 and R125 alone are one phase at (T, p).
#[test]
fn the_binodal_is_where_the_stability_test_changes() {
    let d = diagram().unwrap();
    let at = Components { t: T, p: SPLITS };
    assert_eq!(d.binodal.len(), 2);
    for line in &d.binodal {
        assert!(line.len() >= 10, "{} vertices", line.len());
        for v in line {
            assert_eq!(at.equilibrium(v.outside), Ok(Phases::Single), "{:?}", v.outside);
            let apart = (0..3).map(|i| (v.feed[i] - v.outside[i]).abs()).fold(0.0, f64::max);
            assert!(apart > 0.0 && apart <= RESOLUTION * (1.0 + 1e-9), "{:?}: {apart}", v.feed);
        }
        let ends = [line[0].feed, line[line.len() - 1].feed];
        let edge = |z: [f64; 3]| (0..3).find(|&i| z[i] == 0.0);
        let mut on: Vec<Option<usize>> = ends.into_iter().map(edge).collect();
        on.sort();
        assert_eq!(on, [Some(0), Some(1)], "{ends:?}");
    }
}

/// Whether z lies in a polygon of compositions, in (z₀, z₁), by the crossings of a ray along z₀.
fn inside(polygon: &[[f64; 3]], z: [f64; 3]) -> bool {
    let n = polygon.len();
    (0..n).fold(false, |odd, k| {
        let (a, b) = (polygon[k], polygon[(k + 1) % n]);
        let crosses = (a[1] > z[1]) != (b[1] > z[1]) && z[0] < a[0] + (z[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        odd != crosses
    })
}

/// R407C lies in the diagram's two-phase region at 5 bar, where its flash splits it, and outside it at 4 bar, where its
/// flash leaves it one phase.
#[test]
fn r407c_sits_where_its_flash_says() {
    let single = ternary(&Components { t: T, p: SINGLE }).unwrap();
    for (d, p) in [(diagram().unwrap(), SPLITS), (&single, SINGLE)] {
        let splits = matches!(Components { t: T, p }.equilibrium(R407C), Ok(Phases::Split(_)));
        assert_eq!(splits, p == SPLITS);
        assert!(!d.region.is_empty(), "{p}");
        assert_eq!(d.region.iter().any(|piece| inside(piece, R407C)), splits, "{p}");
    }
}
