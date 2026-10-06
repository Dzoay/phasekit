//! Residual term data the evaluator does not compile yet (map 02 §3.1). Datagen decodes every kind into these from
//! M2.3 on, as each paper prints its terms; each kind's block lands with its step (Lemmon2005 and DoubleExponential
//! M3.3, Gaussian M3.4, GaoB M3.5, NonAnalytic M4.1). Until then a record holding one is refused when it is compiled,
//! never evaluated without it.

/// `n τ^t δ^d e^(−δ^l − τ^m)` (Lemmon & Jacobsen 2005; R125). `l = 0` and `m = 0` mean that factor is absent, as
/// in the paper (CoolProp's "0 means absent", `Helmholtz.cpp:169, 184`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)] // the symbols of the formula above
pub struct Lemmon2005Term {
    pub n: f64,
    pub t: f64,
    pub d: u8,
    pub l: u8,
    pub m: f64,
}

/// `n τ^t δ^d e^(−g_d δ^(l_d) − g_t τ^(l_t))` (de Reuck & Craven 1993; Methanol, where `g_t < 0`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct DoubleExponentialTerm {
    pub n: f64,
    pub t: f64,
    pub d: u8,
    pub gd: f64,
    pub ld: u8,
    pub gt: f64,
    pub lt: f64,
}

/// `n τ^t δ^d e^(−η(δ − ε)² − β(τ − γ)²)`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct GaussianTerm {
    pub n: f64,
    pub t: f64,
    pub d: u8,
    pub eta: f64,
    pub epsilon: f64,
    pub beta: f64,
    pub gamma: f64,
}

/// `n τ^t δ^d e^(−η(δ − ε)² + 1/(β(τ − γ)² + b))` (Gao et al. 2020; Ammonia). `η` has the paper's sign: CoolProp stores
/// −η and evaluates `e^(+η(δ − ε)²)` (`Helmholtz.cpp:643`, map 02 §3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct GaoBTerm {
    pub n: f64,
    pub t: f64,
    pub d: u8,
    pub eta: f64,
    pub epsilon: f64,
    pub beta: f64,
    pub gamma: f64,
    pub b: f64,
}

/// `n Δ^b δ ψ` with `Δ = θ² + B[(δ − 1)²]^a`, `θ = (1 − τ) + A[(δ − 1)²]^(1/(2β))` and
/// `ψ = e^(−C(δ − 1)² − D(τ − 1)²)` (Span & Wagner 1996, Wagner & Pruß 2002; CarbonDioxide, Water).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct NonAnalyticTerm {
    pub n: f64,
    pub a: f64,
    pub b: f64,
    pub beta: f64,
    pub big_a: f64,
    pub big_b: f64,
    pub big_c: f64,
    pub big_d: f64,
}
