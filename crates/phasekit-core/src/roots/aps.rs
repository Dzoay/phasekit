//! Alefeld, Potra & Shi's test problems for [`toms748`] (ACM TOMS 21 (1995) 327-344, §5): the 28 problems, 154 cases
//! with their parameters, as the driver of their ACM Algorithm 748 defines them (netlib `toms/748`, 1995: `driver.f`
//! `INIT` and `FUNC`, `testdata`), and the roots its `testout` prints.

use super::{Stop, Tol, toms748};
use crate::num::math;

/// Problem `nprob` with parameter `n`: its interval and function, as `INIT` and `FUNC` define them (π is the
/// driver's 3.1416).
#[expect(clippy::disallowed_methods, reason = "problems 1, 18 and 27 are sin x; the core itself never calls sin")]
#[expect(clippy::approx_constant, reason = "the driver's own π, 3.1416, which sets problems 1 and 27's intervals")]
fn problem(nprob: u32, n: i32) -> (f64, f64, Box<dyn Fn(f64) -> f64>) {
    let pi = 3.1416;
    let dn = f64::from(n);
    let (a, b) = match nprob {
        1 => (pi / 2.0, pi),
        2..=11 => {
            let i = f64::from(nprob - 1);
            (i * i + 1e-9, (i + 1.0) * (i + 1.0) - 1e-9)
        }
        12..=14 => (-9.0, 31.0),
        15 | 16 => (0.0, 5.0),
        17 => (-0.95, 4.05),
        18 => (0.0, 1.5),
        19..=23 => (0.0, 1.0),
        24 => (1e-2, 1.0),
        25 => (1.0, 100.0),
        26 => (-1.0, 4.0),
        _ => (-10000.0, if nprob == 27 { pi / 2.0 } else { 1e-4 }),
    };
    let f: Box<dyn Fn(f64) -> f64> = match nprob {
        1 => Box::new(|x: f64| x.sin() - x / 2.0),
        2..=11 => Box::new(|x: f64| {
            let sum = (1..=20)
                .map(f64::from)
                .fold(0.0, |sum, i| sum + math::powi(2.0 * i - 5.0, 2) / math::powi(x - i * i, 3));
            -2.0 * sum
        }),
        12 => Box::new(|x: f64| -40.0 * x * math::exp(-x)),
        13 => Box::new(|x: f64| -100.0 * x * math::exp(-2.0 * x)),
        14 => Box::new(|x: f64| -200.0 * x * math::exp(-3.0 * x)),
        15 => Box::new(move |x: f64| math::powi(x, n) - 0.2),
        16 | 17 => Box::new(move |x: f64| math::powi(x, n) - 1.0),
        18 => Box::new(|x: f64| x.sin() - 0.5),
        19 => Box::new(move |x: f64| 2.0 * x * math::exp(-dn) - 2.0 * math::exp(-dn * x) + 1.0),
        20 => Box::new(move |x: f64| (1.0 + math::powi(1.0 - dn, 2)) * x - math::powi(1.0 - dn * x, 2)),
        21 => Box::new(move |x: f64| math::powi(x, 2) - math::powi(1.0 - x, n)),
        22 => Box::new(move |x: f64| (1.0 + math::powi(1.0 - dn, 4)) * x - math::powi(1.0 - dn * x, 4)),
        23 => Box::new(move |x: f64| (x - 1.0) * math::exp(-dn * x) + math::powi(x, n)),
        24 => Box::new(move |x: f64| (dn * x - 1.0) / ((dn - 1.0) * x)),
        25 => Box::new(move |x: f64| math::powf(x, 1.0 / dn) - math::powf(dn, 1.0 / dn)),
        26 => Box::new(|x: f64| if x == 0.0 { 0.0 } else { x / math::exp(1.0 / (x * x)) }),
        27 => Box::new(move |x: f64| if x >= 0.0 { (x / 1.5 + x.sin() - 1.0) * dn / 20.0 } else { -dn / 20.0 }),
        _ => Box::new(move |x: f64| {
            if x >= 1e-3 * 2.0 / (dn + 1.0) {
                math::exp(1.0) - 1.859
            } else if x >= 0.0 {
                math::exp((dn + 1.0) * 0.5 * x * 1e3) - 1.859
            } else {
                -0.859
            }
        }),
    };
    (a, b, f)
}

/// The absolute tolerances w of the runs: 0 (the driver's `NEPS = 1000`: termination at 2·2ε·|u|), 1e-7, 1e-10 and
/// 1e-15.
const WIDTHS: [f64; 4] = [0.0, 1e-7, 1e-10, 1e-15];

/// Per case: the problem, its parameter, the root `testout` prints (14 digits; problem 26's is where x·e^(−1/x²)
/// underflows to 0, the problems 12-14 print 0), and, for the 43 cases built from arithmetic and `powi` alone (so
/// the same on every target), the evaluations at each of [`WIDTHS`] (counting f(a) and f(b)). The counts are
/// Algorithm 748's own: its `RROOT`, `BRACKT`, `NEWQUA`, `PZERO` and `TOLE` (with 10⁻ᴺᴱᴾˢ replaced by w), run on
/// these problems from a line-by-line transliteration that reproduces all 154 printed roots, counted 2026-10-08;
/// the transliteration is not committed (ACM's CALGO licence).
#[rustfmt::skip]
const CASES: [(u32, i32, &str, Option<[usize; 4]>); 154] = [
    (1, 1, "1.8954942670340", None),
    (2, 1, "3.0229153472731", Some([16, 15, 15, 16])),
    (3, 1, "6.6837535608081", Some([12, 8, 9, 11])),
    (4, 1, "11.238701655002", Some([16, 15, 15, 16])),
    (5, 1, "19.676000080623", Some([15, 13, 13, 15])),
    (6, 1, "29.828227326505", Some([13, 12, 12, 13])),
    (7, 1, "41.906116195289", Some([14, 13, 14, 14])),
    (8, 1, "55.953595800143", Some([12, 13, 13, 12])),
    (9, 1, "71.985665586588", Some([14, 13, 13, 14])),
    (10, 1, "90.008868539167", Some([12, 13, 13, 12])),
    (11, 1, "110.02653274833", Some([13, 12, 13, 13])),
    (12, 1, "0.0", None),
    (13, 1, "0.0", None),
    (14, 1, "0.0", None),
    (15, 4, "0.66874030497642", Some([16, 15, 15, 16])),
    (15, 6, "0.76472449133173", Some([19, 18, 18, 18])),
    (15, 8, "0.81776543395794", Some([19, 18, 18, 19])),
    (15, 10, "0.85133992252078", Some([18, 16, 18, 18])),
    (15, 12, "0.87448527222117", Some([18, 16, 18, 18])),
    (16, 4, "1.0000000000000", Some([13, 12, 12, 13])),
    (16, 6, "1.0000000000000", Some([13, 12, 13, 13])),
    (16, 8, "1.0000000000000", Some([16, 14, 15, 16])),
    (16, 10, "1.0000000000000", Some([18, 18, 18, 18])),
    (16, 12, "1.0000000000000", Some([18, 17, 18, 18])),
    (17, 8, "1.0000000000000", Some([22, 21, 21, 22])),
    (17, 10, "1.0000000000000", Some([22, 20, 21, 22])),
    (17, 12, "1.0000000000000", Some([22, 21, 22, 22])),
    (17, 14, "1.0000000000000", Some([24, 24, 24, 24])),
    (18, 1, "0.52359877559830", None),
    (19, 1, "0.42247770964124", None),
    (19, 2, "0.30669941048320", None),
    (19, 3, "0.22370545765466", None),
    (19, 4, "0.17171914751951", None),
    (19, 5, "0.13825715505682", None),
    (19, 20, "3.4657359020854e-02", None),
    (19, 40, "1.7328679513999e-02", None),
    (19, 60, "1.1552453009332e-02", None),
    (19, 80, "8.6643397569993e-03", None),
    (19, 100, "6.9314718055995e-03", None),
    (20, 5, "3.8402551840622e-02", Some([9, 8, 8, 8])),
    (20, 10, "9.9000099980005e-03", Some([7, 6, 7, 7])),
    (20, 20, "2.4937500390620e-03", Some([8, 5, 6, 8])),
    (21, 2, "0.50000000000000", Some([3, 3, 3, 3])),
    (21, 5, "0.34595481584824", Some([11, 9, 9, 11])),
    (21, 10, "0.24512233375331", Some([11, 9, 11, 11])),
    (21, 15, "0.19554762353657", Some([12, 11, 11, 12])),
    (21, 20, "0.16492095727644", Some([12, 11, 11, 12])),
    (22, 1, "0.27550804099948", Some([9, 8, 8, 9])),
    (22, 2, "0.13775402049974", Some([11, 9, 11, 11])),
    (22, 4, "1.0305283778156e-02", Some([11, 9, 9, 11])),
    (22, 5, "3.6171081789041e-03", Some([11, 9, 9, 11])),
    (22, 8, "4.1087291849640e-04", Some([10, 8, 9, 9])),
    (22, 15, "2.5989575892908e-05", Some([9, 8, 8, 9])),
    (22, 20, "7.6685951221853e-06", Some([9, 8, 8, 9])),
    (23, 1, "0.40105813754155", None),
    (23, 5, "0.51615351875793", None),
    (23, 10, "0.53952222690842", None),
    (23, 15, "0.54818229434066", None),
    (23, 20, "0.55270466667849", None),
    (24, 2, "0.50000000000000", Some([14, 13, 13, 14])),
    (24, 5, "0.20000000000000", Some([19, 16, 17, 17])),
    (24, 15, "6.6666666666667e-02", Some([20, 18, 19, 19])),
    (24, 20, "5.0000000000000e-02", Some([18, 18, 18, 18])),
    (25, 2, "2.0000000000000", None),
    (25, 3, "3.0000000000000", None),
    (25, 4, "4.0000000000000", None),
    (25, 5, "5.0000000000000", None),
    (25, 6, "6.0000000000000", None),
    (25, 7, "7.0000000000000", None),
    (25, 9, "9.0000000000000", None),
    (25, 11, "11.000000000000", None),
    (25, 13, "13.000000000000", None),
    (25, 15, "15.000000000000", None),
    (25, 17, "17.000000000000", None),
    (25, 19, "19.000000000000", None),
    (25, 21, "21.000000000000", None),
    (25, 23, "23.000000000000", None),
    (25, 25, "25.000000000000", None),
    (25, 27, "27.000000000000", None),
    (25, 29, "29.000000000000", None),
    (25, 31, "31.000000000000", None),
    (25, 33, "33.000000000000", None),
    (26, 1, "2.2317679157465e-02", None),
    (27, 1, "0.62380651896161", None),
    (27, 2, "0.62380651896161", None),
    (27, 3, "0.62380651896161", None),
    (27, 4, "0.62380651896161", None),
    (27, 5, "0.62380651896161", None),
    (27, 6, "0.62380651896161", None),
    (27, 7, "0.62380651896161", None),
    (27, 8, "0.62380651896161", None),
    (27, 9, "0.62380651896161", None),
    (27, 10, "0.62380651896161", None),
    (27, 11, "0.62380651896161", None),
    (27, 12, "0.62380651896161", None),
    (27, 13, "0.62380651896161", None),
    (27, 14, "0.62380651896161", None),
    (27, 15, "0.62380651896161", None),
    (27, 16, "0.62380651896161", None),
    (27, 17, "0.62380651896161", None),
    (27, 18, "0.62380651896161", None),
    (27, 19, "0.62380651896161", None),
    (27, 20, "0.62380651896161", None),
    (27, 21, "0.62380651896161", None),
    (27, 22, "0.62380651896161", None),
    (27, 23, "0.62380651896161", None),
    (27, 24, "0.62380651896161", None),
    (27, 25, "0.62380651896161", None),
    (27, 26, "0.62380651896161", None),
    (27, 27, "0.62380651896161", None),
    (27, 28, "0.62380651896161", None),
    (27, 29, "0.62380651896161", None),
    (27, 30, "0.62380651896161", None),
    (27, 31, "0.62380651896161", None),
    (27, 32, "0.62380651896161", None),
    (27, 33, "0.62380651896161", None),
    (27, 34, "0.62380651896161", None),
    (27, 35, "0.62380651896161", None),
    (27, 36, "0.62380651896161", None),
    (27, 37, "0.62380651896161", None),
    (27, 38, "0.62380651896161", None),
    (27, 39, "0.62380651896161", None),
    (27, 40, "0.62380651896161", None),
    (28, 20, "5.9051305594220e-05", None),
    (28, 21, "5.6367155339937e-05", None),
    (28, 22, "5.3916409455592e-05", None),
    (28, 23, "5.1669892394942e-05", None),
    (28, 24, "4.9603096699145e-05", None),
    (28, 25, "4.7695285287639e-05", None),
    (28, 26, "4.5928793239949e-05", None),
    (28, 27, "4.4288479195665e-05", None),
    (28, 28, "4.2761290257883e-05", None),
    (28, 29, "4.1335913915954e-05", None),
    (28, 30, "4.0002497338020e-05", None),
    (28, 31, "3.8752419296207e-05", None),
    (28, 32, "3.7578103559958e-05", None),
    (28, 33, "3.6472865219959e-05", None),
    (28, 34, "3.5430783356532e-05", None),
    (28, 35, "3.4446594929961e-05", None),
    (28, 36, "3.3515605877800e-05", None),
    (28, 37, "3.2633616249437e-05", None),
    (28, 38, "3.1796856858426e-05", None),
    (28, 39, "3.1001935436965e-05", None),
    (28, 40, "3.0245790670210e-05", None),
    (28, 100, "1.2277994232462e-05", None),
    (28, 200, "6.1695393904409e-06", None),
    (28, 300, "4.1198585298293e-06", None),
    (28, 400, "3.0924623877272e-06", None),
    (28, 500, "2.4752044261050e-06", None),
    (28, 600, "2.0633567678513e-06", None),
    (28, 700, "1.7690120078154e-06", None),
    (28, 800, "1.5481615698859e-06", None),
    (28, 900, "1.3763345366022e-06", None),
    (28, 1000, "1.2388385788997e-06", None),
];

/// D6 (map 03 §3.4, §6): TOMS 748 is Alefeld, Potra & Shi's Algorithm 4.2 as their Algorithm 748 runs it. On all
/// 154 cases of their test set, at each of [`WIDTHS`], it converges, to the root `testout` prints (within half a
/// unit of its 14th digit, from the end with the smaller |f|; problems 12-14 at w > 0 to within the bracket of 0;
/// problem 26 at an exact zero of f); on the 43 cases that are the same on every target it takes exactly
/// Algorithm 748's evaluations: it evaluates the same points, bit for bit (checked against the transliteration for
/// all 616 runs; problem 26 differs, where the product of differences Algorithm 748 tests for four distinct f values
/// underflows and [`toms748`] compares them pairwise).
#[test]
fn toms748_is_algorithm_748() {
    let mut checked = 0;
    for (nprob, n, printed, counts) in CASES {
        let (a, b, f) = problem(nprob, n);
        let want: f64 = printed.parse().unwrap_or(f64::NAN);
        for (k, w) in WIDTHS.into_iter().enumerate() {
            if want == 0.0 && w == 0.0 {
                continue; // a root at 0 has no relative scale: w = 0 asks for a bracket of a few ulps of 0
            }
            let mut evaluations = 0;
            let root = toms748(
                |x| {
                    evaluations += 1;
                    f(x)
                },
                a,
                b,
                Tol::Absolute(w),
                100,
            );
            let case = format!("problem {nprob} (n = {n}) at w = {w:e}: {root:?} after {evaluations}");
            assert_eq!(root.stop, Stop::Converged, "{case}");
            if let Some(counts) = counts {
                assert_eq!(evaluations, counts[k], "{case}");
            }
            if nprob == 26 {
                assert_eq!(root.f, 0.0, "{case}");
            } else if want == 0.0 {
                assert!(root.x.abs() <= 2.0 * w, "{case}");
            } else if w == 0.0 {
                let digit = math::powi(10.0, math::ln(want.abs()).div_euclid(math::ln(10.0)) as i32 - 13);
                assert!((root.x - want).abs() <= 0.5 * digit + 4.0 * f64::EPSILON * want.abs(), "{case}: {printed}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 154 * 4 - 3);
}
