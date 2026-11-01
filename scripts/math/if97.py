"""IAPWS-IF97 in 50-digit arithmetic: DIV-0065's arbiter and region 5 below 611.213 Pa.

    LC_ALL=C uv run --no-project --python 3.12 --with mpmath==1.3.0 python -I scripts/math/if97.py

DIV-0065: region 1's Gibbs function, IAPWS R7-97(2012) Eq. (7) with Table 2's coefficients, read from
crates/phasekit-iapws/src/if97/region1.rs; the saturation pressure from Eq. (30) with Table 34's; w from Table 3's
relation. Prints p_s and w at 623.15 K to 20 significant digits (PLAN.md M14.3; crates/phasekit-verify/tests/
divergences.rs).

Region 5: Eq. (32) with Tables 37 and 38's coefficients, read from crates/phasekit-iapws/src/if97/region5.rs, and
Table 39's relations. Prints v, h, u, s, c_p and w in SI units to 20 significant digits at Table 42's three states,
which check the transcription, and at 1 and 100 Pa, below where v8.0.0 stops (PLAN.md M27.1, ROT-242;
crates/phasekit-verify/tests/iapws.rs).
"""

import re
import sys
from pathlib import Path

from mpmath import log, mp, mpf, sqrt

mp.dps = 50
ROOT = Path(sys.argv[0]).resolve().parents[2]
SOURCE = (ROOT / "crates/phasekit-iapws/src/if97/region1.rs").read_text()
TERMS = [(int(i), int(j), mpf(n.replace("_", ""))) for i, j, n in
         re.findall(r"^\s+\((-?\d+), (-?\d+), (-?[0-9._e-]+)\),$", SOURCE, re.M)]
assert len(TERMS) == 34, len(TERMS)
N = [mpf(x) for x in ["0.11670521452767e4", "-0.72421316703206e6", "-0.17073846940092e2", "0.12020824702470e5",
                      "-0.32325550322333e7", "0.14915108613530e2", "-0.48232657361591e4", "0.40511340542057e6",
                      "-0.23855557567849", "0.65017534844798e3"]]


def saturation_pressure(t):
    """Eq. (30)."""
    n1, n2, n3, n4, n5, n6, n7, n8, n9, n10 = N
    theta = t + n9 / (t - n10)
    a, b, c = theta**2 + n1 * theta + n2, n3 * theta**2 + n4 * theta + n5, n6 * theta**2 + n7 * theta + n8
    return mpf(10)**6 * (2 * c / (-b + sqrt(b * b - 4 * a * c)))**4


def speed_of_sound(t, p):
    """Table 3: w² = RT γ_π² / ((γ_π − τγ_πτ)²/(τ²γ_ττ) − γ_ππ), from Table 4's derivatives of Eq. (7)."""
    pi, tau = p / mpf("16.53e6"), mpf(1386) / t
    x, y = mpf("7.1") - pi, tau - mpf("1.222")
    g_p = g_pp = g_tt = g_pt = mpf(0)
    for i, j, n in TERMS:
        term = n * x**i * y**j
        g_p += -i * term / x
        g_pp += i * (i - 1) * term / x**2
        g_tt += j * (j - 1) * term / y**2
        g_pt += -i * j * term / (x * y)
    return sqrt(mpf("461.526") * t * g_p**2 / ((g_p - tau * g_pt)**2 / (tau**2 * g_tt) - g_pp))


T = mpf("623.15")
P = saturation_pressure(T)
print("p_s", mp.nstr(P, 20))
print("w", mp.nstr(speed_of_sound(T, P), 20))


REGION5 = (ROOT / "crates/phasekit-iapws/src/if97/region5.rs").read_text()
IDEAL5 = [(int(j), mpf(n.replace("_", ""))) for j, n in
          re.findall(r"^\s+\((-?\d+), (-?[0-9._e-]+)\),$", REGION5, re.M)]
RESIDUAL5 = [(int(i), int(j), mpf(n.replace("_", ""))) for i, j, n in
             re.findall(r"^\s+\((-?\d+), (-?\d+), (-?[0-9._e-]+)\),$", REGION5, re.M)]
assert (len(IDEAL5), len(RESIDUAL5)) == (6, 6), (len(IDEAL5), len(RESIDUAL5))
R = mpf("461.526")


def region5(t, p):
    """Table 39: v, h, u, s, c_p and w from γ⁰ (Eq. (33)) and γ^r (Eq. (34)) and their derivatives, Tables 40-41."""
    pi, tau = p / mpf(10)**6, mpf(1000) / t
    g0 = log(pi) + sum(n * tau**j for j, n in IDEAL5)
    g0_t = sum(n * j * tau**(j - 1) for j, n in IDEAL5)
    g0_tt = sum(n * j * (j - 1) * tau**(j - 2) for j, n in IDEAL5)
    gr = sum(n * pi**i * tau**j for i, j, n in RESIDUAL5)
    gr_p = sum(n * i * pi**(i - 1) * tau**j for i, j, n in RESIDUAL5)
    gr_pp = sum(n * i * (i - 1) * pi**(i - 2) * tau**j for i, j, n in RESIDUAL5)
    gr_t = sum(n * pi**i * j * tau**(j - 1) for i, j, n in RESIDUAL5)
    gr_tt = sum(n * pi**i * j * (j - 1) * tau**(j - 2) for i, j, n in RESIDUAL5)
    gr_pt = sum(n * i * pi**(i - 1) * j * tau**(j - 1) for i, j, n in RESIDUAL5)
    g0_p = 1 / pi
    v = R * t / p * pi * (g0_p + gr_p)
    h = R * t * tau * (g0_t + gr_t)
    u = R * t * (tau * (g0_t + gr_t) - pi * (g0_p + gr_p))
    s = R * (tau * (g0_t + gr_t) - (g0 + gr))
    cp = -R * tau**2 * (g0_tt + gr_tt)
    a = 1 + pi * gr_p - tau * pi * gr_pt
    w2 = R * t * (1 + 2 * pi * gr_p + pi**2 * gr_p**2) / ((1 - pi**2 * gr_pp) + a**2 / (tau**2 * (g0_tt + gr_tt)))
    return v, h, u, s, cp, sqrt(w2)


for t, p in [("1500", "0.5e6"), ("1500", "30e6"), ("2000", "30e6"), ("1500", "1"), ("2000", "100")]:
    print("region5", t, "K", p, "Pa", " ".join(mp.nstr(x, 20) for x in region5(mpf(t), mpf(p))))
