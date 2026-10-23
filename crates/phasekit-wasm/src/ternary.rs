//! A three-component mixture's ternary diagram at one T and p (PLAN.md M26.5, M26.6): phasekit-diagram's binodal, tie
//! lines and two-phase region, its seam filled from phasekit-mix's PT equilibrium, as typed arrays in mole fractions,
//! three a point, so that a page draws the triangle and samples nothing itself.

use phasekit_core::{Basis, DataSet, Error, Input, Pair};
use phasekit_diagram::{self as diagram, Phases, Split, TernaryBatch};
use phasekit_mix::Equilibrium;
use wasm_bindgen::prelude::*;

use crate::diagram::offset;
use crate::{Registry, alive, js, mixture_of, refuse};

/// A mixture's components at (T, p) in a registry, each feed's equilibrium phasekit-mix's: its non-zero components'
/// mixture's, or a pure component's PT state, one phase.
struct Components<'a> {
    registry: &'a phasekit_core::Registry,
    names: Vec<String>,
    t: f64,
    p: f64,
}

impl Components<'_> {
    fn equilibrium(&self, z: [f64; 3]) -> Result<Phases, Error> {
        let parts: Vec<(&str, f64)> = self.names.iter().map(String::as_str).zip(z).collect();
        let fluid = phasekit_mix::mixture(self.registry, &parts, DataSet::Corrected)?;
        let Some(mixture) = mixture_of(&fluid) else {
            fluid.state(Input::new(Pair::PT, self.p, self.t, Basis::Mass)?)?;
            return Ok(Phases::Single);
        };
        let present: Vec<usize> = z.iter().enumerate().filter(|(_, x)| **x > 0.0).map(|(i, _)| i).collect();
        let whole = |part: &[f64]| {
            let mut out = [0.0; 3];
            for (&i, &x) in present.iter().zip(part) {
                if let Some(o) = out.get_mut(i) {
                    *o = x;
                }
            }
            out
        };
        match mixture.equilibrium(self.t, self.p)? {
            Equilibrium::Split { beta, x, y, .. } => {
                Ok(Phases::Split(Split { beta, liquid: whole(&x), vapour: whole(&y) }))
            }
            _ => Ok(Phases::Single),
        }
    }
}

impl TernaryBatch for Components<'_> {
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

/// A ternary diagram, as flat arrays in mole fractions, three a point in the components' order (`components`):
/// the binodal's line `k` is the points of `binodal` from `lines[k]` to `lines[k + 1]`, each a feed that splits, its
/// vapour fraction `binodalBeta`, `outside` the feed at its bracket's other end, one phase; tie line `j` joins `liquid[3j..3j + 3]` and
/// `vapour[3j..3j + 3]`, the phases of the feed `feeds[3j..3j + 3]`, `beta[j]` its vapour's molar fraction; the
/// region's polygon `i` is the points of `region` from `polygons[i]` to `polygons[i + 1]`.
#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq)]
pub struct Ternary {
    components: Vec<String>,
    t: f64,
    p: f64,
    lines: Vec<u32>,
    binodal: Vec<f64>,
    binodal_beta: Vec<f64>,
    outside: Vec<f64>,
    feeds: Vec<f64>,
    beta: Vec<f64>,
    liquid: Vec<f64>,
    vapour: Vec<f64>,
    polygons: Vec<u32>,
    region: Vec<f64>,
}

impl Ternary {
    fn of(d: &diagram::Ternary, components: Vec<String>) -> Result<Ternary, JsError> {
        let mut out = Ternary {
            components,
            t: d.t,
            p: d.p,
            lines: vec![0],
            binodal: Vec::new(),
            binodal_beta: Vec::new(),
            outside: Vec::new(),
            feeds: Vec::new(),
            beta: Vec::new(),
            liquid: Vec::new(),
            vapour: Vec::new(),
            polygons: vec![0],
            region: Vec::new(),
        };
        for line in &d.binodal {
            for v in line {
                out.binodal.extend(v.feed);
                out.binodal_beta.push(v.split.beta);
                out.outside.extend(v.outside);
            }
            out.lines.push(offset(out.binodal.len() / 3)?);
        }
        for tie in &d.tie_lines {
            out.feeds.extend(tie.feed);
            out.beta.push(tie.split.beta);
            out.liquid.extend(tie.split.liquid);
            out.vapour.extend(tie.split.vapour);
        }
        for polygon in &d.region {
            polygon.iter().for_each(|z| out.region.extend(z));
            out.polygons.push(offset(out.region.len() / 3)?);
        }
        Ok(out)
    }
}

#[wasm_bindgen]
impl Ternary {
    /// The components' names, in the order of each point's three fractions.
    #[wasm_bindgen(getter)]
    pub fn components(&self) -> Vec<String> {
        self.components.clone()
    }

    /// The temperature, K.
    #[wasm_bindgen(getter, js_name = T)]
    pub fn t(&self) -> f64 {
        self.t
    }

    /// The pressure, Pa.
    #[wasm_bindgen(getter)]
    pub fn p(&self) -> f64 {
        self.p
    }

    /// Each binodal line's first point, and one past the last's: one more entry than lines.
    #[wasm_bindgen(getter)]
    pub fn lines(&self) -> Vec<u32> {
        self.lines.clone()
    }

    /// The binodal's points, each a feed that splits, three fractions a point.
    #[wasm_bindgen(getter)]
    pub fn binodal(&self) -> Vec<f64> {
        self.binodal.clone()
    }

    /// Each binodal point's vapour fraction: near 0 on the bubble side, where the feed is nearly all liquid, near 1 on
    /// the dew side.
    #[wasm_bindgen(getter, js_name = binodalBeta)]
    pub fn binodal_beta(&self) -> Vec<f64> {
        self.binodal_beta.clone()
    }

    /// Each binodal point's feed outside the region, one phase, within the binodal's resolution of it.
    #[wasm_bindgen(getter)]
    pub fn outside(&self) -> Vec<f64> {
        self.outside.clone()
    }

    /// Each tie line's feed, three fractions a line.
    #[wasm_bindgen(getter)]
    pub fn feeds(&self) -> Vec<f64> {
        self.feeds.clone()
    }

    /// Each tie line's vapour's molar fraction of its feed.
    #[wasm_bindgen(getter)]
    pub fn beta(&self) -> Vec<f64> {
        self.beta.clone()
    }

    /// Each tie line's liquid end.
    #[wasm_bindgen(getter)]
    pub fn liquid(&self) -> Vec<f64> {
        self.liquid.clone()
    }

    /// Each tie line's vapour end.
    #[wasm_bindgen(getter)]
    pub fn vapour(&self) -> Vec<f64> {
        self.vapour.clone()
    }

    /// Each region polygon's first point, and one past the last's: one more entry than polygons.
    #[wasm_bindgen(getter)]
    pub fn polygons(&self) -> Vec<u32> {
        self.polygons.clone()
    }

    /// The two-phase region's polygons' points, each counter-clockwise in the first two fractions.
    #[wasm_bindgen(getter)]
    pub fn region(&self) -> Vec<f64> {
        self.region.clone()
    }
}

#[wasm_bindgen]
impl Registry {
    /// The ternary diagram of the three-component mixture `name` (a mixture name, `R407C.mix` or
    /// `R32[0.4]&R125[0.2]&R134a[0.4]`) at T (K) and p (Pa) (PLAN.md M26.5, M26.6): phasekit-diagram's over the
    /// composition triangle, each feed's equilibrium phasekit-mix's PT flash with the stability test, on the library's
    /// pair parameters; a feed on a side of the triangle is that binary mixture, one at a corner that pure fluid. The
    /// name's own fractions place no point. A fluid that is no three-component mixture is refused.
    pub fn ternary(&self, name: &str, t: f64, p: f64) -> Result<Ternary, JsValue> {
        alive()?;
        let fluid = phasekit_compat::fluid_in(&self.inner, name).map_err(refuse)?;
        let components: Vec<String> = mixture_of(&fluid)
            .map(|m| m.model().fluids().map(|f| f.info().name().to_string()).collect())
            .unwrap_or_default();
        if components.len() != 3 {
            return Err(js(format!("{name} is no three-component mixture: it has no ternary diagram")).into());
        }
        let batch = Components { registry: &self.inner, names: components.clone(), t, p };
        Ok(Ternary::of(&diagram::ternary(&batch).map_err(refuse)?, components)?)
    }
}
