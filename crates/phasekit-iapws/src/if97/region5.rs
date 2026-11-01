//! Region 5, the high-temperature steam: γ(π, τ) = γ⁰ + γ^r with γ⁰ = ln π + Σ n⁰ᵢ τ^J⁰ᵢ (IAPWS R7-97(2012) Eq. (33),
//! Table 37) and γ^r = Σ nᵢ π^Iᵢ τ^Jᵢ (Eq. (34), Table 38), π = p/p* and τ = T*/T, and their derivatives to order 2
//! (Tables 40 and 41), evaluated in the form region 2 shares (`steam`).

use super::steam::Steam;

/// Table 37: (J⁰ᵢ, n⁰ᵢ).
const IDEAL: [(i32, f64); 6] = [
    (0, -0.131_799_836_742_01e2),
    (1, 0.685_408_416_344_34e1),
    (-3, -0.248_051_489_334_66e-1),
    (-2, 0.369_015_349_803_33),
    (-1, -0.311_613_182_139_25e1),
    (2, -0.329_616_265_389_17),
];

/// Table 38: (Iᵢ, Jᵢ, nᵢ).
const RESIDUAL: [(i32, i32, f64); 6] = [
    (1, 1, 0.157_364_048_552_59e-2),
    (1, 2, 0.901_537_616_739_44e-3),
    (1, 3, -0.502_700_776_776_48e-2),
    (2, 3, 0.224_400_374_094_85e-5),
    (2, 9, -0.411_632_754_534_71e-5),
    (3, 7, 0.379_194_548_229_55e-7),
];

/// Region 5: p* = 1 MPa and T* = 1000 K (Eqs. (33) and (34)), τ itself in γ^r.
pub(super) const REGION5: Steam = Steam { p_star: 1e6, t_star: 1000.0, tau_0: 0.0, ideal: &IDEAL, residual: &RESIDUAL };
