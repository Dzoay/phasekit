# M12's exp probe and the policy's name (2026-10-13)

Asked after M12.1. A throwaway 4-lane port of the `libm` crate's `exp` (scratch code, not committed) matched `libm::exp`
bit for bit on 20,000,080 inputs, edge cases included, and took 3.80 ns per lane under AVX2 on the i7-8700K. Scalar
`libm` took 8.1-8.7 ns and glibc 2.98 ns. For n-Propane's 19 transcendentals per state, that estimates the α^r lanes at
about 2.0-2.3× over the scalar path, if everything else vectorises 3-4×: below SL3's 2.5×.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL4 | The probe estimates the gate below 2.5×: prototype it first, build the steps, or close the spike? | **Build the steps as planned** | M12.2-M12.6 are built as production steps, and M12.7 measures the gate, as PLAN.md M12 says. |
| SL5 | The lanes `ExecPolicy` variant's name (M12.3) | **`ExecPolicy::Simd`** | It runs the scalar path where no lane kernel exists or the `simd` feature is off. |
