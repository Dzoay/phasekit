//! Reading and writing by index without a panic (PLAN.md M24.4; issue #219), which the browser package could not catch.
//! The indices these kernels compute cannot leave their vectors; if one did, it would read NaN, which their checks for a
//! finite result refuse, and a write there would be skipped.

/// Entry `i` of `v`, or NaN where it has none.
pub(crate) fn at(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(f64::NAN)
}

/// Entry (`r`, `c`) of the matrix `a`, or NaN where it has none.
pub(crate) fn at2<R: AsRef<[f64]>>(a: &[R], r: usize, c: usize) -> f64 {
    a.get(r).map_or(f64::NAN, |row| at(row.as_ref(), c))
}

/// Sets entry `i` of `v` to `value`, where it has one.
pub(crate) fn set(v: &mut [f64], i: usize, value: f64) {
    if let Some(x) = v.get_mut(i) {
        *x = value;
    }
}
