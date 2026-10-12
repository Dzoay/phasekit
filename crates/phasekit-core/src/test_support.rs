//! What the unit tests share (PLAN.md M7.9d): a sweep over several case lists is one loop over their product, its
//! body a named function, so no test nests deeper than Sonar's bars (`cargo xtask gates complexity`).

/// Every pair of an item of `a` and an item of `b`, `a` outermost: the cases of `for x in a { for y in b { … } }`, in
/// that order.
pub(crate) fn product<A: Clone, B: Clone>(a: &[A], b: &[B]) -> impl Iterator<Item = (A, B)> {
    a.iter().flat_map(move |x| b.iter().map(move |y| (x.clone(), y.clone())))
}
