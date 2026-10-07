//! A fluid file's superancillary → core's `Superancillary` (PLAN.md M5.2; map 03 §3.3, §9). The three expansions are
//! checked (13 coefficients per piece, one set of boundaries for ρ′, ρ″ and p), and what CoolProp builds at load or on
//! first use, under a mutex (map 11 F7), is computed here once and validated:
//!
//! - each curve's extrema: the temperatures where its derivative changes sign, the derivative's coefficients taken with
//!   [`chebyshev_derivative`] (CoolProp drops the last coefficient, ROT-087), sampled densely and bisected;
//! - the T(ln p) inverse: degree-12 pieces over ln p through the Chebyshev-Lobatto nodes, each node's T solved from the
//!   forward p(T) by bisection; a piece that misses the forward curve between nodes by more than [`INVERSE_TOL`] is
//!   halved (CoolProp's target is 1e-12 too, map 03 §3.3).

use phasekit_core::internal::{Superancillary, chebyshev_derivative, clenshaw, piece};
use phasekit_core::math;
use serde_json::Value;

use super::mirror;

/// The degree of every v8.0.0 superancillary piece, and of the inverse (map 03 §3.3).
pub const DEGREE: usize = 12;

/// The largest relative error in T that the inverse may make at the midpoints between its nodes.
pub const INVERSE_TOL: f64 = 1e-12;

/// Derivative samples per piece when looking for extrema.
const SAMPLES: usize = 64;

/// The record's superancillary from the file's `EOS[0].SUPERANCILLARY`.
pub fn superancillary(sa: &mirror::Superancillary) -> Result<Superancillary, String> {
    let (breaks, rho_l) = expansions(&sa.jexpansions_rho_l, "jexpansions_rhoL")?;
    let (breaks_v, rho_v) = expansions(&sa.jexpansions_rho_v, "jexpansions_rhoV")?;
    let (breaks_p, p) = expansions(&sa.jexpansions_p, "jexpansions_p")?;
    if breaks_v != breaks || breaks_p != breaks {
        return Err("SUPERANCILLARY: the rhoL, rhoV and p expansions must share their pieces".into());
    }
    let curves = [rho_l, rho_v, p];
    let extrema = [0, 1, 2].map(|k| extrema(&breaks, &curves[k]));
    // T(p) is single-valued above p's last extremum (PropyleneGlycol's fit dips to a minimum of 2.6523e-8 Pa at
    // 216.54 K, above its 213 K triple point; measured at M5.2), and T(ln p) has a square-root singularity at it, so the
    // inverse starts at the first piece boundary above it; below, PQ solves the forward curve (M6.8).
    let t_start = match extrema[2].last() {
        Some(&t) => breaks.iter().copied().find(|&b| b > t).unwrap_or(breaks[breaks.len() - 1]),
        None => breaks[0],
    };
    let (ln_p_breaks, t_of_ln_p) = inverse(&breaks, &curves[2], t_start)?;
    let sa = Superancillary { breaks, curves, extrema, ln_p_breaks, t_of_ln_p };
    sa.check().map_err(|e| e.to_string())?;
    Ok(sa)
}

/// One `jexpansions_*` list: contiguous pieces `{xmin, xmax, coef[13]}` → the boundaries and the coefficient rows.
fn expansions(value: &Value, name: &str) -> Result<(Vec<f64>, Vec<[f64; 13]>), String> {
    let at = |m: String| format!("SUPERANCILLARY.{name}: {m}");
    let pieces = value.as_array().filter(|p| !p.is_empty()).ok_or_else(|| at("not a list of pieces".into()))?;
    let (mut breaks, mut coefs) = (Vec::new(), Vec::new());
    for (i, piece) in pieces.iter().enumerate() {
        let number = |key: &str| piece.get(key).and_then(Value::as_f64).ok_or_else(|| at(format!("[{i}].{key}")));
        let (xmin, xmax) = (number("xmin")?, number("xmax")?);
        if breaks.last().is_some_and(|&last| last != xmin) {
            return Err(at(format!("[{i}] starts at {xmin} K, not where the last piece ended")));
        }
        if breaks.is_empty() {
            breaks.push(xmin);
        }
        breaks.push(xmax);
        let row = piece.get("coef").and_then(Value::as_array).ok_or_else(|| at(format!("[{i}].coef")))?;
        let row: Vec<f64> =
            row.iter().map(Value::as_f64).collect::<Option<_>>().ok_or_else(|| at(format!("[{i}].coef")))?;
        coefs.push(<[f64; 13]>::try_from(row).map_err(|r| at(format!("[{i}] has {} coefficients, not 13", r.len())))?);
    }
    Ok((breaks, coefs))
}

/// x of [a, b] on [−1, 1], as the runtime maps it.
fn scaled(x: f64, a: f64, b: f64) -> f64 {
    (2.0 * x - (b + a)) / (b - a)
}

/// The curve at T: the piece that holds T, by the runtime's lookup and mapping.
fn eval(breaks: &[f64], curve: &[[f64; 13]], t: f64) -> f64 {
    let Some(i) = piece(breaks, t) else { return f64::NAN };
    clenshaw(&curve[i], scaled(t, breaks[i], breaks[i + 1]))
}

/// The temperatures where the curve's T-derivative changes sign: each piece's derivative sampled at `SAMPLES` + 1
/// points from end to end, and every change of sign between the last nonzero sample and the next one located: at an
/// exact zero between them, by bisection within one piece, or at the boundary when the two samples are the ends of
/// neighbouring pieces.
pub fn extrema(breaks: &[f64], curve: &[[f64; 13]]) -> Vec<f64> {
    let mut found = Vec::new();
    // The last nonzero slope (its piece, x and value) and the first exact zero since it, as T.
    let (mut last, mut zero): (Option<(usize, f64, f64)>, Option<f64>) = (None, None);
    for (i, coef) in curve.iter().enumerate() {
        let (a, b) = (breaks[i], breaks[i + 1]);
        let d = chebyshev_derivative(coef);
        let slope = |x: f64| clenshaw(&d, x);
        let t_of = |x: f64| a + (x + 1.0) * (b - a) / 2.0;
        for k in 0..=SAMPLES {
            let x = -1.0 + 2.0 * k as f64 / SAMPLES as f64;
            let s = slope(x);
            if s == 0.0 {
                zero = zero.or(Some(t_of(x)));
                continue;
            }
            if let Some((j, x0, _)) = last.filter(|&(_, _, s0): &(usize, f64, f64)| s0.signum() != s.signum()) {
                found.push(match zero {
                    Some(t) => t,
                    None if j == i => t_of(bisect(&slope, x0, x)),
                    None => a,
                });
            }
            (last, zero) = (Some((i, x, s)), None);
        }
    }
    // Strictly inside the range: a change of sign needs a nonzero sample on each side of it.
    found
}

/// The root of `f` between `lo` and `hi`, where f changes sign, bisected until the midpoint repeats an end.
fn bisect(f: &impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let f_lo = f(lo);
    loop {
        let mid = lo + (hi - lo) / 2.0;
        if mid <= lo || mid >= hi {
            return mid;
        }
        if f_lo * f(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
}

/// The Chebyshev coefficients of the degree-12 interpolant through `values` at the Chebyshev-Lobatto nodes x_k =
/// cos(πk/12), k = 0…12: c_j = (2/N)·Σ″_k f_k cos(πjk/N), the end terms halved in the sum and c_0, c_N halved after
/// (the discrete cosine transform behind CoolProp's L matrix, map 03 §3.3). M5.2a fits the caloric curves with it.
pub fn lobatto_fit(values: &[f64; 13]) -> [f64; 13] {
    let n = DEGREE as f64;
    let mut c = [0.0; 13];
    for (j, cj) in c.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (k, &f) in values.iter().enumerate() {
            let weight = if k == 0 || k == DEGREE { 0.5 } else { 1.0 };
            sum += weight * f * math::cos(std::f64::consts::PI * (j * k) as f64 / n);
        }
        *cj = 2.0 / n * sum * if j == 0 || j == DEGREE { 0.5 } else { 1.0 };
    }
    c
}

/// The Chebyshev-Lobatto node k of [a, b]: a value of cos(πk/12) mapped from [−1, 1].
pub fn lobatto_node(k: usize, a: f64, b: f64) -> f64 {
    let x = math::cos(std::f64::consts::PI * k as f64 / DEGREE as f64);
    (a + b) / 2.0 + x * (b - a) / 2.0
}

/// The deepest dyadic split of one p piece's ln p range (CoolProp stops after 26 passes, map 03 §3.3).
const MAX_SPLITS: u32 = 26;

/// The T(ln p) inverse from `t_lo` (where p starts to increase for good) to the top of the range. It starts from the p
/// pieces: piece i spans ln p from the start of T piece i (by that piece) to the start of the next (by that one), the
/// last to the end of the range, so the ln p boundaries increase with no gaps. Each node's T solves ln p(T) = y by
/// bisection over [t_lo, top]. A piece whose fit misses the solved T at a
/// midpoint between nodes by more than [`INVERSE_TOL`] relative is halved in ln p, up to [`MAX_SPLITS`] times
/// (near the triple point a p piece can span several decades of p).
pub fn inverse(breaks: &[f64], p: &[[f64; 13]], t_lo: f64) -> Result<(Vec<f64>, Vec<[f64; 13]>), String> {
    inverse_within(breaks, p, t_lo, INVERSE_TOL, MAX_SPLITS)
}

/// [`inverse`] to relative error `tol` with at most `max_splits` halvings of a piece.
fn inverse_within(
    breaks: &[f64],
    p: &[[f64; 13]],
    t_lo: f64,
    tol: f64,
    max_splits: u32,
) -> Result<(Vec<f64>, Vec<[f64; 13]>), String> {
    let t_hi = breaks[breaks.len() - 1];
    let ln_p = |t: f64| math::ln(eval(breaks, p, t));
    let interior = breaks[..breaks.len() - 1].iter().filter(|&&t| t > t_lo);
    let mut starts: Vec<f64> = std::iter::once(t_lo).chain(interior.copied()).map(ln_p).collect();
    starts.push(ln_p(t_hi));
    if !starts.windows(2).all(|w| w[0] < w[1]) {
        return Err("SUPERANCILLARY: ln p at the piece boundaries does not increase".into());
    }
    let solve = |y: f64| bisect(&|t| ln_p(t) - y, t_lo, t_hi);
    // The fit of [a, b] and its largest relative miss at the midpoints between nodes.
    let fit = |a: f64, b: f64| {
        let row = lobatto_fit(&std::array::from_fn(|k| solve(lobatto_node(k, a, b))));
        let miss = (0..DEGREE)
            .map(|k| (lobatto_node(k, a, b) + lobatto_node(k + 1, a, b)) / 2.0)
            .map(|y| (clenshaw(&row, scaled(y, a, b)) / solve(y) - 1.0).abs())
            .fold(0.0, f64::max);
        (row, miss)
    };
    let (mut ln_p_breaks, mut rows) = (vec![starts[0]], Vec::new());
    let mut pending: Vec<(f64, f64, u32)> = starts.windows(2).rev().map(|w| (w[0], w[1], 0)).collect();
    while let Some((a, b, depth)) = pending.pop() {
        let (row, miss) = fit(a, b);
        if miss <= tol {
            ln_p_breaks.push(b);
            rows.push(row);
        } else if depth < max_splits {
            let mid = a + (b - a) / 2.0;
            pending.extend([(mid, b, depth + 1), (a, mid, depth + 1)]);
        } else {
            return Err(format!("SUPERANCILLARY: the T(ln p) inverse misses by {miss:e} on ln p in [{a}, {b}]"));
        }
    }
    Ok((ln_p_breaks, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interpolant reproduces a polynomial of degree ≤ 12 exactly: T_5 at the nodes gives c_5 = 1, the rest 0,
    /// and a constant gives c_0 alone.
    #[test]
    fn lobatto_fit_recovers_chebyshev_coefficients() {
        let t5 = |x: f64| 16.0 * math::powi(x, 5) - 20.0 * math::powi(x, 3) + 5.0 * x;
        let values: [f64; 13] = std::array::from_fn(|k| t5(lobatto_node(k, -1.0, 1.0)));
        let c = lobatto_fit(&values);
        for (j, cj) in c.iter().enumerate() {
            assert!((cj - if j == 5 { 1.0 } else { 0.0 }).abs() < 1e-14, "c_{j} = {cj}");
        }
        let flat = lobatto_fit(&[2.5; 13]);
        assert!((flat[0] - 2.5).abs() < 1e-14 && flat[1..].iter().all(|c| c.abs() < 1e-14), "{flat:?}");
        assert_eq!([lobatto_node(0, 2.0, 4.0), lobatto_node(12, 2.0, 4.0)], [4.0, 2.0]);
        assert!((lobatto_node(6, 2.0, 4.0) - 3.0).abs() < 1e-15);
    }

    /// A parabola with its vertex inside the second of two pieces, and one whose vertex is the shared boundary: one
    /// extremum each, where it is; a monotone line has none.
    #[test]
    fn extrema_are_sign_changes_of_the_derivative() {
        // f = (T − 3.3)² on [2, 3] and [3, 5], written in each piece's x: T = 2.5 + x/2 and T = 4 + x.
        let piece_of = |a: f64, b: f64, vertex: f64| {
            let (m, h) = ((a + b) / 2.0, (b - a) / 2.0);
            // (m + h·x − v)² = (m − v)² + 2h(m − v)x + h²x² = c0 + c1 T1 + c2 T2 with x² = (T2 + 1)/2.
            let mut c = [0.0; 13];
            (c[0], c[1], c[2]) = ((m - vertex) * (m - vertex) + h * h / 2.0, 2.0 * h * (m - vertex), h * h / 2.0);
            c
        };
        let breaks = [2.0, 3.0, 5.0];
        let found = extrema(&breaks, &[piece_of(2.0, 3.0, 3.3), piece_of(3.0, 5.0, 3.3)]);
        assert_eq!(found.len(), 1);
        assert!((found[0] - 3.3).abs() < 1e-12, "{found:?}");
        assert_eq!(extrema(&breaks, &[piece_of(2.0, 3.0, 3.0), piece_of(3.0, 5.0, 3.0)]), vec![3.0]);
        let mut line = [0.0; 13];
        line[1] = 1.0;
        assert!(extrema(&breaks, &[line, line]).is_empty());
    }

    /// A file's superancillary from JSON pieces: `p` is given per piece as (T − 3)² + 1, written in each piece's x.
    fn json(breaks: &[f64], p_breaks: &[f64]) -> mirror::Superancillary {
        let pieces = |breaks: &[f64], f: &dyn Fn(f64, f64) -> [f64; 13]| {
            let list: Vec<serde_json::Value> = breaks
                .windows(2)
                .map(|w| serde_json::json!({"xmin": w[0], "xmax": w[1], "coef": f(w[0], w[1]).to_vec()}))
                .collect();
            serde_json::Value::Array(list)
        };
        let constant = |value: f64| {
            move |_: f64, _: f64| -> [f64; 13] { std::array::from_fn(|j| if j == 0 { value } else { 0.0 }) }
        };
        let parabola = |a: f64, b: f64| -> [f64; 13] {
            let (m, h) = ((a + b) / 2.0, (b - a) / 2.0);
            std::array::from_fn(|j| match j {
                0 => (m - 3.0) * (m - 3.0) + h * h / 2.0 + 1.0,
                1 => 2.0 * h * (m - 3.0),
                2 => h * h / 2.0,
                _ => 0.0,
            })
        };
        let value = serde_json::json!({
            "jexpansions_rhoL": pieces(breaks, &constant(1000.0)),
            "jexpansions_rhoV": pieces(breaks, &constant(1.0)),
            "jexpansions_p": pieces(p_breaks, &parabola),
            "crit_anc": {}, "meta": {},
        });
        serde_json::from_value(value).unwrap()
    }

    /// The three curves must share their pieces; p's extremum at the boundary 3 K starts the inverse at the next
    /// boundary, 4 K, where T(ln p) is smooth (at 3 K it is not, and no fit would pass).
    #[test]
    fn pieces_are_shared_and_the_inverse_starts_above_p_extrema() {
        let breaks = [2.0, 3.0, 4.0, 5.0];
        let sa = superancillary(&json(&breaks, &breaks)).unwrap();
        assert_eq!(sa.extrema[2], vec![3.0]);
        assert_eq!(sa.ln_p_breaks[0], math::ln(2.0)); // p(4 K) = 1 + 1
        let shared = "SUPERANCILLARY: the rhoL, rhoV and p expansions must share their pieces";
        assert_eq!(superancillary(&json(&breaks, &[2.0, 3.5, 4.0, 5.0])).unwrap_err(), shared);
        // p = T² on [1, 148.4]: T(ln p) = e^(ln p / 2), which one degree-12 piece over ln p in [0, 10] misses by 5e-8,
        // halves of it by 2.3e-12 and quarters by 4.3e-13 (measured): at 1e-12 it takes exactly two levels of splits.
        let (a, b) = (1.0, 148.4);
        let (m, h) = ((a + b) / 2.0, (b - a) / 2.0);
        let square: [f64; 13] =
            std::array::from_fn(|j| [m * m + h * h / 2.0, 2.0 * m * h, h * h / 2.0].get(j).copied().unwrap_or(0.0));
        let within =
            |max_splits| inverse_within(&[a, b], &[square], a, 1e-12, max_splits).map(|(breaks, _)| breaks.len() - 1);
        assert!(within(1).unwrap_err().starts_with("SUPERANCILLARY: the T(ln p) inverse misses by "));
        assert!(within(2).is_ok_and(|pieces| pieces >= 3), "{:?}", within(2));
        // A flat p has no extremum, but its ln p boundaries do not increase, so no inverse exists.
        let flat: [f64; 13] = std::array::from_fn(|j| if j == 0 { 5.0 } else { 0.0 });
        let err = inverse(&breaks, &[flat; 3], breaks[0]).unwrap_err();
        assert_eq!(err, "SUPERANCILLARY: ln p at the piece boundaries does not increase");
    }

    /// The 130 v8.0.0 superancillaries (PLAN.md M5.2): the extrema found are the density maxima of liquid water and
    /// heavy water, PropyleneGlycol's dip in p above its triple point, and wiggles of two ρ″ fits within 3e-5 K of the
    /// critical point; no other curve has one. The stored inverse returns T from ln p(T) within [`INVERSE_TOL`] at
    /// every T piece's midpoint above its start, and covers the range above p's last extremum.
    #[test]
    fn superancillaries_have_their_extrema_and_inverse() {
        let sources = super::super::load(&crate::repo::Repo::locate()).unwrap();
        let mut with_extrema = Vec::new();
        for source in &sources {
            let Some(json) = &source.fluid.eos[0].superancillary else { continue };
            let sa = superancillary(json).unwrap();
            let (breaks, top) = (&sa.breaks, sa.breaks[sa.breaks.len() - 1]);
            for (curve, found) in ["rho'", "rho''", "p"].iter().zip(&sa.extrema) {
                if !found.is_empty() {
                    with_extrema.push((source.fluid.info.name.as_str(), *curve, found.clone()));
                }
            }
            let t_start = bisect(&|t| math::ln(eval(breaks, &sa.curves[2], t)) - sa.ln_p_breaks[0], breaks[0], top);
            for w in breaks.windows(2).filter(|w| w[0] >= t_start) {
                let t = (w[0] + w[1]) / 2.0;
                let y = math::ln(eval(breaks, &sa.curves[2], t));
                let back = eval(&sa.ln_p_breaks, &sa.t_of_ln_p, y);
                assert!((back / t - 1.0).abs() <= INVERSE_TOL, "{}: T(ln p({t} K)) = {back} K", source.file);
            }
            let p_last = sa.extrema[2].last().copied().unwrap_or(breaks[0]);
            assert!(t_start >= p_last && breaks.iter().any(|&b| b == t_start || (b - t_start).abs() < 1e-9 * b));
        }
        let near = |found: &[f64], t: f64, within: f64| found.iter().all(|x| (x - t).abs() < within);
        let names: Vec<(&str, &str, usize)> = with_extrema.iter().map(|(n, c, f)| (*n, *c, f.len())).collect();
        assert_eq!(
            names,
            [
                ("DimethylCarbonate", "rho''", 4),
                ("HeavyWater", "rho'", 1),
                ("PropyleneGlycol", "p", 1),
                ("Water", "rho'", 1),
                ("m-Xylene", "rho''", 2)
            ]
        );
        for (name, _, found) in &with_extrema {
            let at = match *name {
                "Water" => (277.150, 1e-3),
                "HeavyWater" => (284.768, 1e-3),
                "PropyleneGlycol" => (216.539, 1e-3),
                "DimethylCarbonate" => (557.0, 2e-5),
                _ => (616.89, 3e-5),
            };
            assert!(near(found, at.0, at.1), "{name}: {found:?}");
        }
    }
}
