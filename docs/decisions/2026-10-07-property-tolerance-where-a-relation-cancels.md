# Property tolerance where a relation cancels (2026-10-07)

Asked at M5.1 (PLAN.md §0.4 stop: a gate could pass only by changing a tolerance class). Comparing properties at (T, ρ)
with the oracle, 7 of 80,880 per-PR entries (c_p, w, (∂p/∂ρ)_T next to a spinodal or just outside the near-critical
window) and 584 of 13.5 M nightly entries (p and Z at Z ≈ 1e-4; h, u, s, c_v, c_p, w in deep metastable liquid, R22 at
130 K and −138 MPa) missed `Prop` (1e-12) by up to 127×. Both codes agree there only as far as their α derivatives
do, which `Term` already verifies. The user first approved a floor for the three outputs that inherit (∂p/∂ρ)_T's
error; when the nightly grid showed cancellation inside α^r itself, they approved the general rule below instead.

| # | Question | Decision | Notes |
|---|---|---|---|
| TC1 | How `Prop` treats outputs whose relation cancels | **`Prop` bound = max(1e-12 · max(\|v\|, floor) (nc 1e-8), `Term` carried through the relation)** | The carried scale is Σ_ij \|∂X/∂A_ij\|·M_ij with M_ij the `Term` scales of the order-2 bundle (`phasekit_verify::eos::carried`); no new constant. Measured at M5.1: 0 of 13.6 M entries fail, headroom 0.036; the strict bound is the larger for 89 % of entries and 99 % lie within 15× of it. VERIFICATION.md §5 states the rule; principle 4 ("measured conditioning") and `Identity`'s largest-term scale are its precedents. |
