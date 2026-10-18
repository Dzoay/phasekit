# M12's lane widths (2026-10-14)

Asked at M12.7. M12.4's plan ran the lanes one vector wide (8 on AVX-512, 4 on AVX2, 2 below). With that width,
n-Propane's α^r took 215 ns per state on the i7-8700K, 1.46× over M12.1's 312.9 ns, against SL3's 2.5×. α^r's work in
a lane is chains of dependent multiply-adds, and one vector's latency leaves the pipelines idle. A width sweep at one,
two, four and eight vectors gave 213, 120, 109 and 125 ns on AVX2, and 356, 252, 184 and 192 ns on SSE4.2.

| # | Question | Decision | Notes |
|---|---|---|---|
| SL6 | Run the lanes four vectors wide, a deviation from M12.4's table, or record the gate as failed? | **Four vectors to a lane operation** | The lanes are 32 wide on AVX-512 (unmeasured), 16 on AVX2, and 8 on SSE2, SSE4.2, NEON and simd128. A batch's last partial group runs one vector wide. Each lane is still its state's scalar evaluation, so no bits change. n-Propane's α^r takes 109.2 ns, 2.87× over M12.1, so SL3's gate passes; it misses ≤ 0.1 µs/state by 9 % (issue #171). *Rejected:* M12.4's one vector, which fails the gate. |
