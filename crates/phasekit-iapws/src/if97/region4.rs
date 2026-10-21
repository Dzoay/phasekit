//! Region 4, the saturation line: the implicit quadratic Eq. (29) of IAPWS R7-97(2012), solved for the pressure
//! (Eq. (30)) and for the temperature (Eq. (31)), with p* = 1 MPa and T* = 1 K; its coefficients are Table 34's.

use phasekit_core::math;

/// Table 34: n₁ … n₁₀.
const N: [f64; 10] = [
    0.116_705_214_527_67e4,
    -0.724_213_167_032_06e6,
    -0.170_738_469_400_92e2,
    0.120_208_247_024_70e5,
    -0.323_255_503_223_33e7,
    0.149_151_086_135_30e2,
    -0.482_326_573_615_91e4,
    0.405_113_405_420_57e6,
    -0.238_555_575_678_49,
    0.650_175_348_447_98e3,
];

/// p_s at `t` (K), Pa: Eq. (30) with ϑ of Eq. (29b).
pub(super) fn pressure(t: f64) -> f64 {
    let [n1, n2, n3, n4, n5, n6, n7, n8, n9, n10] = N;
    let theta = t + n9 / (t - n10);
    let a = theta * theta + n1 * theta + n2;
    let b = n3 * theta * theta + n4 * theta + n5;
    let c = n6 * theta * theta + n7 * theta + n8;
    let x = 2.0 * c / (-b + math::sqrt(b * b - 4.0 * a * c));
    1e6 * (x * x) * (x * x)
}

/// T_s at `p` (Pa), K: Eq. (31) with β of Eq. (29a).
pub(super) fn temperature(p: f64) -> f64 {
    let [n1, n2, n3, n4, n5, n6, n7, n8, n9, n10] = N;
    let beta = math::sqrt(math::sqrt(p / 1e6));
    let e = beta * beta + n3 * beta + n6;
    let f = n1 * beta * beta + n4 * beta + n7;
    let g = n2 * beta * beta + n5 * beta + n8;
    let d = 2.0 * g / (-f - math::sqrt(f * f - 4.0 * e * g));
    (n10 + d - math::sqrt((n10 + d) * (n10 + d) - 4.0 * (n9 + n10 * d))) / 2.0
}
