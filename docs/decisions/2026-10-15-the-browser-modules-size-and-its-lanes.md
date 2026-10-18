# The browser module's size and its lanes (2026-10-15)

Asked at M12.9. The browser `.wasm` had one budget for both builds, 1 184 227 B (M10.8's first release size + 10 %).
M12.9's QT and PQ lanes, which `ExecPolicy::Simd` reaches on every build, take the baseline to 1 188 898 B. M12.10
enables `simd` in the `simd128` build, whose size grows with the lane width. Under Node 26 on the i7-8700K, n-Propane's
DT + h + c_p, QT + h and PQ + h batches against the same build's scalar path: 2 lanes 1 221 205 B, 1.30×, 1.22× and
1.17×; 4 lanes, padded, 1 288 342 B, 1.37×, 1.37× and 1.33×; 8 lanes, padded, 1 325 552 B, 1.50×, 1.53× and 1.45×;
SL6's 8 lanes with a one-vector remainder 1 402 513 B, 1.25×, 1.62× and 1.45×. Without `simd` the `simd128` build is
1 165 725 B.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL8 | The browser `.wasm`'s budget, and the lane width of its `simd128` build | **Eight lanes with the last group padded; a budget per build, its measured size + 10 %** | The `simd128` build enables `simd` and runs 8 lanes, a short last group padded as K17 pads it, with no one-vector remainder: on wasm a deviation from SL6, smaller and faster. The budgets: baseline 1 307 788 B (M12.9's 1 188 898 + 10 %), `simd128` 1 458 107 B (1 325 552 + 10 %). The browser package's batch runs `ExecPolicy::Simd` on both builds, with the same bits. *Rejected:* 2 or 4 lanes (smaller, slower); no `simd` in the browser. |
