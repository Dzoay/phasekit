# The lanes' expm1 (2026-10-14)

Asked at M12.8, which runs α⁰'s transcendentals other than `exp` and `ln` per lane through `math` unless a measurement
justifies a port. On the i7-8700K with `libm`, `expm1` takes 11.1 ns a call, and n-Propane's four Planck-Einstein terms
call it twice each: about 89 ns of its 129 ns of α⁰ in lanes. 114 of the 136 fluids have such terms.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL7 | Port `expm1` to lanes in M12.8? | **Yes** | The `libm` crate's `expm1` ported like M12.5's `exp`, operation for operation, bitwise. `pow` stays per lane: libm's is far larger to port. |
