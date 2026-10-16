#!/usr/bin/env python3
"""The CoolProp 8.0.0 oracle fixture generator (docs/VERIFICATION.md section 3.1).

Standard library plus CoolProp only, so the uv environment is exactly the pinned wheel. `cargo xtask oracle` runs it
in a scrubbed environment:

    env -i PATH="$PATH" HOME="$HOME" LC_ALL=C TZ=UTC PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 \
      uv run --no-project --python 3.12 --with CoolProp==8.0.0 \
      python scripts/oracle/gen.py --lock crates/phasekit-verify/fixtures/oracle.lock --kind facts --set smoke --out DIR

Five start-up assertions run before any row is computed; any failure exits non-zero. A kind is added here in the
PLAN.md step that first uses it.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import re
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FLUIDS_DIR = ROOT / "reference" / "CoolProp" / "dev" / "fluids"
MIXTURES_DIR = ROOT / "reference" / "CoolProp" / "dev" / "mixtures"
FIXTURES_DIR = ROOT / "crates" / "phasekit-verify" / "fixtures"
MANIFEST = FIXTURES_DIR / "MANIFEST.sha256"
KINDS = ["facts", "checkpoints", "term", "eos", "crit", "sat", "flash", "transport", "sigma", "melt", "refstate", "cubic",
         "cubic_sat", "mix_alphar", "mix_eos", "mix_crit", "mix_flash", "props", "codes"]
SCRUBBED_PREFIXES = ("COOLPROP_", "PXFLASH_")
FLUID_COUNT = 136

# The committed core subset (VERIFICATION.md section 3.6): one file per fluid and kind. The all-fluid tier holds every
# fluid in one file per kind under all/.
CORE_FLUIDS = ["Air", "Ammonia", "CarbonDioxide", "HFE143m", "Helium", "Methanol", "Nitrogen", "R1130(E)", "R1234yf",
               "R1234ze(E)", "R125", "R410A", "Water", "n-Heptane"]

# splitmix64-v1, seed 1: the vector phasekit_verify::sample checks too (VERIFICATION.md section 3.1, assertion 5).
GOLDEN = [0x910A2DEC89025CC1, 0xBEEB8DA1658EEC67, 0xF893A2EEFB32555E, 0x71C18690EE42C90B]

# The error classes of a failed call, from a fixed message table; messages are never stored (section 3.1).
ERROR_CLASSES = [
    ("notimpl", ("not available", "not implemented")),
    ("solver", ("iterations", "no solution", "solver failed")),
    ("domain", ("out of range", "must be", "outside")),
]

# The `facts` sets (section 3.5): one named oracle fact per row. `call` is written with `;` because cells hold no
# commas. A call is `PropsSI`, `Props1SI` or `AbstractState` (see `abstract_state`).
THOL_2016_TABLE3 = [(0, 200, 12600), (1, 350, 11400), (2, 383, 4290), (4, 360, 1000), (5, 420, 8000)]  # row, K, mol/m3
NIST_IR_8474_TABLE3 = [(0, 4, 40000), (1, 4, 2000), (2, 10, 50000), (3, 10, 2000), (4, 300, 25000), (5, 300, 1000)]
VIRIAL_STATES = [("propane", "n-Propane", 300), ("nitrogen", "Nitrogen", 300), ("water", "Water", 600)]
BELL_TABLE_XI_MIXTURE = "HEOS::R1234yf[0.4]&R1234ze(E)[0.6]"  # z1 = 0.4 at T = 469 K, 3399 mol/m3 (map 10 section 8.4)
# What a saturated phase gives at Q = 0 and 1, as a single phase (DIV-0004: c_p, c_v and w since SQ1, the partials and
# order-3 outputs since SQ2, c_p0 and the residual h, s and g since SQ3).
SATURATED_OUTPUTS = [("cpmass", "CPMASS"), ("cvmass", "CVMASS"), ("w", "A"), ("dhmass_dp_t", "d(Hmass)/d(P)|T"),
                     ("d2p_drho_t_dt_rho", "d(d(P)/d(Dmolar)|T)/d(T)|Dmolar"),
                     ("kappa_t", "isothermal_compressibility"), ("beta", "isobaric_expansion_coefficient"),
                     ("gamma", "fundamental_derivative_of_gas_dynamics"), ("cp0mass", "CP0MASS"),
                     ("hmolar_residual", "HMOLAR_RESIDUAL"), ("smolar_residual", "SMOLAR_RESIDUAL"),
                     ("gmolar_residual", "GMOLAR_RESIDUAL")]
FACTS = {
    "smoke": [
        ("r134a_h_t300_q1", "PropsSI", ("H", "T", 300, "Q", 1, "R134a")),
        ("water_tcrit", "Props1SI", ("Water", "Tcrit")),
        ("water_t_reducing", "Props1SI", ("Water", "T_reducing")),
    ],
    # The oracle side of every divergence register entry (PLAN.md M1.13; map 12 section 6.3, map 10 section 8.4,
    # map 13 section 3). Names start with the entry's id; rows of a paper table carry the row's index in its file.
    "register": [
        ("div0001_gas_constant", "Props1SI", ("R1234ze(E)", "gas_constant")),
        *[
            (f"div0001_p_row{i}", "PropsSI", ("P", "T", t, "Dmolar", rho, "R1234ze(E)"))
            for i, t, rho in THOL_2016_TABLE3
        ],
        ("div0002_melting_t_1356.76mpa", "AbstractState", ("HEOS", "Water", "melting_line", "iT", "iP", 1356.76e6)),
        ("div0002_melting_p_320k", "AbstractState", ("HEOS", "Water", "melting_line", "iP", "iT", 320)),
        ("div0003_rhomolar_reducing", "Props1SI", ("Nitrogen", "rhomolar_reducing")),
        ("div0004_eta_t500_q0.5", "PropsSI", ("V", "T", 500, "Q", 0.5, "Water")),
        # The saturated outputs at Q = 0 and 1 on Water's 500 K isotherm and Air's 101325 Pa isobar (a pseudo-pure
        # fluid's dew point at its own T); strictly inside the dome, the EOS at the mean density, Water's c_p and
        # residual h and CO2's kappa_T, and Air's c_p0 at the dome's T.
        *[
            (f"div0004_{name}_{label}_q{q}", "PropsSI", (key, *given, "Q", q, fluid))
            for label, fluid, given in (("water_t500", "Water", ("T", 500)), ("air_p101325", "Air", ("P", 101325)))
            for q in (0, 1)
            for name, key in SATURATED_OUTPUTS
        ],
        ("div0004_cpmass_water_t500_q0.5", "PropsSI", ("CPMASS", "T", 500, "Q", 0.5, "Water")),
        ("div0004_kappa_t_co2_t280_q0.5", "PropsSI",
         ("isothermal_compressibility", "T", 280, "Q", 0.5, "CarbonDioxide")),
        ("div0004_hmolar_residual_water_t500_q0.5", "PropsSI", ("HMOLAR_RESIDUAL", "T", 500, "Q", 0.5, "Water")),
        ("div0004_cp0mass_air_p101325_q0.5", "PropsSI", ("CP0MASS", "P", 101325, "Q", 0.5, "Air")),
        ("div0005_gas_constant", "Props1SI", ("Helium", "gas_constant")),
        *[
            (f"div0005_{name}_row{i}", "PropsSI", (key, "T", t, "Dmolar", rho, "Helium"))
            for i, t, rho in NIST_IR_8474_TABLE3
            for name, key in (("p", "P"), ("cv", "CVMOLAR"), ("w", "A"))
        ],
        ("div0006_rhomolar_reducing", "Props1SI", ("Ethylene", "rhomolar_reducing")),
        ("div0006_molar_mass", "Props1SI", ("Ethylene", "molar_mass")),
        ("div0007_rhomolar_reducing", "Props1SI", ("OrthoHydrogen", "rhomolar_reducing")),
        ("div0007_molar_mass", "Props1SI", ("OrthoHydrogen", "molar_mass")),
        ("div0008_rhomolar_reducing", "Props1SI", ("n-Undecane", "rhomolar_reducing")),
        ("div0009_eta_t300_p101325", "PropsSI", ("V", "T", 300, "P", 101325, "R1233zd(E)")),
        ("div0010_smolar_t399.99", "PropsSI", ("Smolar", "T", 399.99, "P", 1e5, "PR::n-Propane")),
        ("div0010_smolar_t400.01", "PropsSI", ("Smolar", "T", 400.01, "P", 1e5, "PR::n-Propane")),
        ("div0010_cpmolar_t400", "PropsSI", ("CPMOLAR", "T", 400, "P", 1e5, "PR::n-Propane")),
        *[
            fact
            for label, fluid, t in VIRIAL_STATES
            for fact in [
                (f"div0011_cvirial_{label}", "PropsSI", ("Cvirial", "T", t, "Dmolar", 0.001, fluid)),
                (f"div0011_rhomolar_reducing_{label}", "Props1SI", (fluid, "rhomolar_reducing")),
                *[
                    (f"div0011_d2alphar_ddelta2_{label}_rho{rho}", "AbstractState",
                     ("HEOS", fluid, "DmolarT_INPUTS", float(rho), t, "d2alphar_dDelta2"))
                    for rho in (1, 2, 3, 4)
                ],
            ]
        ],
        ("div0012_p_t250_rho55018.5", "PropsSI", ("P", "T", 250, "Dmolar", 55018.5, "Water")),
        ("div0013_alphar_t469_rho3399", "PropsSI", ("alphar", "T", 469, "Dmolar", 3399, BELL_TABLE_XI_MIXTURE)),
        ("div0014_p_t400_rho8000", "PropsSI", ("P", "T", 400, "Dmolar", 8000, "R1224YDZ")),
        # Tkaczuk et al. 2020 Table 8's states, the oracle's p with its R_CODATA (DIV-0051).
        *[
            (f"div0051_p_{label}", "PropsSI", ("P", "T", 200, "Dmolar", 10000, f"HEOS::{a}[0.5]&{b}[0.5]"))
            for label, a, b in (("he_ne", "Helium", "Neon"), ("he_ar", "Helium", "Argon"), ("ne_ar", "Neon", "Argon"))
        ],
        # Lemmon and Jacobsen 2004 Table 6's R32/R134a liquid row, the oracle's c_v (DIV-0053: R134a's critical T is not
        # its reducing T, and v8.0.0 evaluates its alpha0 at the critical one in a mixture).
        ("div0053_cvmolar_r32_r134a_t300_rho15000", "PropsSI",
         ("CVMOLAR", "T", 300, "Dmolar", 15000, "HEOS::R32[0.5]&R134a[0.5]")),
        ("div0015_t_reducing", "Props1SI", ("R123", "T_reducing")),
        ("div0015_cp0molar_t300", "PropsSI", ("CP0MOLAR", "T", 300, "Dmolar", 1, "R123")),
        # CoolProp's own VLE at PropyleneGlycol's triple point, superancillaries off (DIV-0016).
        ("div0016_rhomolar_liquid_t213", "PropsSIVle", ("Dmolar", "T", 213, "Q", 0, "PropyleneGlycol")),
        ("div0016_rhomolar_vapour_t213", "PropsSIVle", ("Dmolar", "T", 213, "Q", 1, "PropyleneGlycol")),
        # CoolProp's in-dome p of SES36 at one T and two densities: it changes with Q (DIV-0019).
        ("div0019_p_t206.2675_rho1.97058", "PropsSI", ("P", "T", 206.2675, "Dmolar", 1.97058, "SES36")),
        ("div0019_p_t206.2675_rho0.656961", "PropsSI", ("P", "T", 206.2675, "Dmolar", 0.656961, "SES36")),
        # CoolProp's eta at the transport check rows its correlations miss beyond their printed digits (DIV-0021: the
        # papers' own equations; DIV-0022: the rows' densities, printed to fewer digits than eta needs).
        ("div0021_eta_sf6_t300_rho0", "PropsSI", ("V", "T", 300, "Dmass", 1e-14, "SulfurHexafluoride")),
        ("div0021_eta_sf6_t400_rho0", "PropsSI", ("V", "T", 400, "Dmass", 1e-14, "SulfurHexafluoride")),
        ("div0021_eta_r125_t300", "PropsSI", ("V", "T", 300, "Dmolar", 10596.9998, "R125")),
        ("div0022_eta_ammonia_t200", "PropsSI", ("V", "T", 200, "Dmolar", 42754.4, "Ammonia")),
        ("div0022_eta_r123_t265", "PropsSI", ("V", "T", 265, "Dmass", 1545.8, "R123")),
        ("div0022_eta_noctane_t300", "PropsSI", ("V", "T", 300, "Dmolar", 6177.2, "n-Octane")),
        ("div0022_eta_npropane_t150", "PropsSI", ("V", "T", 150, "Dmolar", 15140, "n-Propane")),
        # CoolProp's lambda at the conductivity check rows off their printed digits (DIV-0023: near the critical point
        # and a coarse density; DIV-0024: Friend's methane; DIV-0025: n-Pentane's critical term, with the viscosity
        # it reads).
        ("div0023_lambda_air_t132.64", "PropsSI", ("L", "T", 132.64, "Dmolar", 10400, "Air")),
        ("div0023_lambda_argon_t150.69", "PropsSI", ("L", "T", 150.69, "Dmolar", 13400, "Argon")),
        ("div0023_lambda_nitrogen_t126.195", "PropsSI", ("L", "T", 126.195, "Dmolar", 11180, "Nitrogen")),
        ("div0023_lambda_oxygen_t154.6", "PropsSI", ("L", "T", 154.6, "Dmolar", 13600, "Oxygen")),
        ("div0023_lambda_sf6_t310", "PropsSI", ("L", "T", 310, "Dmass", 1200, "SulfurHexafluoride")),
        ("div0023_lambda_nonane_t300", "PropsSI", ("L", "T", 300, "Dmolar", 5619.4, "n-Nonane")),
        ("div0024_lambda_methane_t400", "PropsSI", ("L", "T", 400, "Dmass", 1e-12, "Methane")),
        ("div0024_lambda_methane_t182_q0", "PropsSI", ("L", "T", 182, "Q", 0, "Methane")),
        ("div0024_lambda_methane_t100", "PropsSI", ("L", "T", 100, "Dmolar", 28800, "Methane")),
        ("div0025_lambda_npentane_t460", "PropsSI", ("L", "T", 460, "Dmass", 377.687, "n-Pentane")),
        ("div0025_eta_npentane_t460", "PropsSI", ("V", "T", 460, "Dmass", 377.687, "n-Pentane")),
        # CoolProp at M8.9's check rows (DIV-0027: o-xylene's Table 6 against its Eq. (12); DIV-0030: Ammonia's Tufeu
        # term at its own Tc; DIV-0031: the papers' own correlations; DIV-0032: the viscosity the critical term reads;
        # DIV-0033: Friend's ethane; DIV-0034: the papers not obtained; DIV-0035: Helium's critical term, NaN in two
        # lobes either side of its critical isochore).
        ("div0027_lambda_oxylene_t300", "PropsSI", ("L", "T", 300, "Dmass", 1e-12, "o-Xylene")),
        ("div0030_lambda_ammonia_t405.3999_rho50", "PropsSI", ("L", "T", 405.3999, "Dmass", 50, "Ammonia")),
        ("div0030_lambda_ammonia_t405.4_rho50", "PropsSI", ("L", "T", 405.4, "Dmass", 50, "Ammonia")),
        ("div0031_eta_benzene_t300", "PropsSI", ("V", "T", 300, "Dmass", 875, "Benzene")),
        ("div0031_eta_heptane_t250", "PropsSI", ("V", "T", 250, "Dmass", 720, "n-Heptane")),
        ("div0031_eta_methanol_t300", "PropsSI", ("V", "T", 300, "Dmass", 788.41, "Methanol")),
        ("div0032_lambda_cyclopentane_t512", "PropsSI", ("L", "T", 512, "Dmass", 400, "Cyclopentane")),
        ("div0032_eta_cyclopentane_t512", "PropsSI", ("V", "T", 512, "Dmass", 400, "Cyclopentane")),
        ("div0032_lambda_isopentane_t460", "PropsSI", ("L", "T", 460, "Dmass", 329.914, "Isopentane")),
        ("div0032_eta_isopentane_t460", "PropsSI", ("V", "T", 460, "Dmass", 329.914, "Isopentane")),
        ("div0032_lambda_heptane_t535", "PropsSI", ("L", "T", 535, "Dmass", 100, "n-Heptane")),
        ("div0032_eta_heptane_t535", "PropsSI", ("V", "T", 535, "Dmass", 100, "n-Heptane")),
        ("div0032_lambda_hexane_t400", "PropsSI", ("L", "T", 400, "Dmass", 650, "n-Hexane")),
        ("div0033_eta_ethane_t100", "PropsSI", ("V", "T", 100, "Dmolar", 21330, "Ethane")),
        ("div0033_lambda_ethane_t440", "PropsSI", ("L", "T", 440, "Dmolar", 1520, "Ethane")),
        ("div0034_eta_r23_t180", "PropsSI", ("V", "T", 180, "Dmolar", 21097, "R23")),
        ("div0034_eta_co2_t300", "PropsSI", ("V", "T", 300, "Dmass", 1400, "CarbonDioxide")),
        ("div0034_lambda_r123_t180", "PropsSI", ("L", "T", 180, "Dmass", 0.002873, "R123")),
        ("div0035_lambda_helium_t5.5_rho17000", "AbstractState",
         ("HEOS", "Helium", "DmolarT_INPUTS", 17000, 5.5, "conductivity")),
        ("div0035_lambda_helium_t5.5_rho18000", "AbstractState",
         ("HEOS", "Helium", "DmolarT_INPUTS", 18000, 5.5, "conductivity")),
        ("div0035_lambda_helium_t5.9_rho15000", "AbstractState",
         ("HEOS", "Helium", "DmolarT_INPUTS", 15000, 5.9, "conductivity")),
        # Hydrogen's and OrthoHydrogen's melting line at their triple points, beside their triple pressures, and at
        # 20 K beside ParaHydrogen's (DIV-0036; map 02 section 6 row 5).
        ("div0036_melting_p_hydrogen_t13.957", "AbstractState",
         ("HEOS", "Hydrogen", "melting_line", "iP", "iT", 13.957)),
        ("div0036_ptriple_hydrogen", "Props1SI", ("Hydrogen", "ptriple")),
        ("div0036_melting_p_orthohydrogen_t14.008", "AbstractState",
         ("HEOS", "OrthoHydrogen", "melting_line", "iP", "iT", 14.008)),
        ("div0036_ptriple_orthohydrogen", "Props1SI", ("OrthoHydrogen", "ptriple")),
        ("div0036_melting_p_hydrogen_t20", "AbstractState", ("HEOS", "Hydrogen", "melting_line", "iP", "iT", 20)),
        ("div0036_melting_p_parahydrogen_t20", "AbstractState",
         ("HEOS", "ParaHydrogen", "melting_line", "iP", "iT", 20)),
        ("div0037_q_water_t400_rho10", "PropsSI", ("Q", "T", 400, "Dmolar", 10, "Water")),
        ("div0037_qmass_r407c_t291.68", "PropsSI",
         ("Qmass", "T", 291.67568518763323, "Dmolar", 405.544225835316, "R407C")),
        ("div0038_gwp100_r134a", "Props1SI", ("R134a", "GWP100")),
        ("div0039_rhomass_reducing_water", "Props1SI", ("Water", "rhomass_reducing")),
        ("div0040_t_triple_ammonia", "Props1SI", ("Ammonia", "T_triple")),
        ("div0041_sigma_r125_t318.0", "PropsSI",
         ("surface_tension", "T", 318.00780422661575, "Dmolar", 532.3155734056108, "R125")),
        ("div0042_tcrit_air_q0.5_t100", "PropsSI", ("T_critical", "Q", 0.5, "T", 100, "Air")),
        ("div0042_h_air_q0.5_t100", "PropsSI", ("Hmolar", "Q", 0.5, "T", 100, "Air")),
        ("div0043_g_air_q0_t80", "PropsSI", ("Gmolar", "Q", 0, "T", 80, "Air")),
        ("div0043_h_air_q0_t80", "PropsSI", ("Hmolar", "Q", 0, "T", 80, "Air")),
        ("div0044_z_r1234yf_t203.94", "PropsSI",
         ("Z", "T", 203.94070458008517, "Dmolar", 803.4844557660671, "R1234yf")),
        *[
            (f"div0044_{key.lower()}_propyleneglycol_q1_t225.99", "PropsSI",
             (key, "Q", 1, "T", 225.99105497362368, "PropyleneGlycol"))
            for key in ("Z", "P", "Dmolar")
        ],
        ("div0044_gas_constant_propyleneglycol", "Props1SI", ("PropyleneGlycol", "gas_constant")),
        ("div0045_eta_propylene_q1_t104.87", "PropsSI", ("viscosity", "Q", 1, "T", 104.86761486209713, "Propylene")),
        ("div0045_lambda_r14_q1_t215.07", "PropsSI", ("conductivity", "Q", 1, "T", 215.06807556754023, "R14")),
        ("div0046_gas_constant_pr_water", "Props1SI", ("PR::Water", "gas_constant")),
        *[
            (f"div0046_{name}_{fluid.lower()}_t{t}", "PropsSI", (key, "T", t, "P", 1e4, prefix + fluid))
            for fluid, t in (("n-Propane", 300), ("Water", 400), ("CycloPropane", 300))
            for name, key, prefix in (("gas_constant", "gas_constant", ""), ("cp0molar_heos", "CP0MOLAR", ""),
                                      ("cp0molar_pr", "CP0MOLAR", "PR::"))
        ],
        # DIV-0047: at the exact critical point (T_c, p_c/(Z_c R T_c)) of n-Propane's SRK and PR, with v8.0.0's own
        # T_c, p_c and R_U_CODATA, Z should be Z_c and dp/drho zero.
        *[
            (f"div0047_{name}_{kind.lower()}_n-propane", "PropsSI",
             (key, "T", 369.89, "Dmolar", 4251200.0 / (z_c * 8.31446261815324 * 369.89), f"{kind}::n-Propane"))
            for kind, z_c in (("SRK", 1.0 / 3.0), ("PR", 0.30740130869870386))
            for name, key in (("z", "Z"), ("dpdrho", "d(P)/d(Dmolar)|T"))
        ],
        # DIV-0048: v8.0.0's cubic library's T_c, p_c and omega for Neon and D5; DIV-0049: its alpha0 for R40 and R134a
        # against the HEOS fluid's at one gas state.
        *[
            (f"div0048_{key.lower()}_pr_{fluid.lower()}", "Props1SI", (f"PR::{fluid}", key))
            for fluid in ("Neon", "D5")
            for key in ("Tcrit", "pcrit", "acentric")
        ],
        *[
            (f"div0049_{name}_{prefix.strip(':').lower() or 'heos'}_{fluid.lower()}_t500", "PropsSI",
             (key, "T", 500.0, "P", 1e4, prefix + fluid))
            for fluid, name, key in (("R40", "hmolar", "Hmolar"), ("R134a", "cp0molar", "CP0MOLAR"))
            for prefix in ("PR::", "")
        ],
    ],
}


def fail(message):
    print(f"gen.py: {message}", file=sys.stderr)
    sys.exit(2)


def read_lock(path):
    """`key value` lines (section 3.2); `#` lines are comments."""
    lock = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        key, _, value = line.partition(" ")
        if key in lock:
            fail(f"{path}: {key} appears twice")
        lock[key] = value.strip()
    return lock


def assert_environment():
    """Assertion 1, before `import CoolProp`: no load-time switch can be set, and the locale is C."""
    switches = sorted(k for k in os.environ if k.startswith(SCRUBBED_PREFIXES))
    if switches:
        fail(f"assertion 1: these variables must not be set: {', '.join(switches)}")
    if os.environ.get("LC_ALL") != "C":
        fail("assertion 1: LC_ALL must be C")


def import_coolprop(lock):
    """Assertion 2: the wheel is the pinned one."""
    import CoolProp
    import CoolProp.CoolProp as CP

    if CoolProp.__version__ != lock["version"]:
        fail(f"assertion 2: CoolProp {CoolProp.__version__}, the lock pins {lock['version']}")
    if CoolProp.__gitrevision__ != lock["git"]:
        fail(f"assertion 2: git {CoolProp.__gitrevision__}, the lock pins {lock['git']}")
    so = Path(CoolProp.__file__).parent / lock["so_name"]
    if not so.is_file() or hashlib.sha256(so.read_bytes()).hexdigest() != lock["so_sha256"]:
        fail(f"assertion 2: {so} is missing or its sha256 differs from the lock")
    if len(CP.FluidsList()) != FLUID_COUNT:
        fail(f"assertion 2: {len(CP.FluidsList())} fluids, not {FLUID_COUNT}")
    return CP


def set_config(CP, lock):
    """Assertion 3: every config key set explicitly to the lock's value, then read back equal."""
    wanted = json.loads(lock["config_json"])
    current = json.loads(CP.get_config_as_json_string())
    if set(wanted) != set(current):
        fail(f"assertion 3: the lock's config_json must name exactly the wheel's {len(current)} keys")
    setters = [(bool, CP.set_config_bool), (int, CP.set_config_int), (float, CP.set_config_double),
               (str, CP.set_config_string)]
    for name, value in sorted(wanted.items()):
        setter = next(set_ for type_, set_ in setters if isinstance(value, type_))
        try:
            setter(getattr(CP.configuration_keys, name), value)
        except (TypeError, ValueError) as exception:
            fail(f"assertion 3: cannot set {name} to {value!r}: {exception}")
    if json.loads(CP.get_config_as_json_string()) != wanted:
        fail("assertion 3: the config read back differs from the lock")
    return canonical_json(wanted)


def fluid_files(lock):
    """Fluid name -> its file in reference/CoolProp/dev/fluids (names come from INFO.NAME, not file names), after
    checking the directory against the lock's fluids_sha256 (the sorted "file sha256" lines; part of assertion 4)."""
    paths = sorted(FLUIDS_DIR.glob("*.json"))
    listing = "".join(sorted(f"{p.name} {hashlib.sha256(p.read_bytes()).hexdigest()}\n" for p in paths))
    if hashlib.sha256(listing.encode("utf-8")).hexdigest() != lock["fluids_sha256"]:
        fail(f"assertion 4: {FLUIDS_DIR} differs from the lock's fluids_sha256 (scripts/fetch-coolprop.sh)")
    return {json.loads(p.read_text(encoding="utf-8"))["INFO"]["NAME"]: p for p in paths}


def mixture_library(lock):
    """The binary pairs and predefined mixtures of reference/CoolProp/dev/mixtures, after checking the directory's four
    files (data/mixtures.lock's) against the lock's mixtures_sha256 (the sorted "file sha256" lines; assertion 4)."""
    names = ["ASHRAE_predefined_2026.tsv", "mixture_binary_pairs.json", "mixture_departure_functions.json",
             "predefined_mixtures.json"]
    listing = "".join(f"{n} {hashlib.sha256((MIXTURES_DIR / n).read_bytes()).hexdigest()}\n" for n in names)
    if hashlib.sha256(listing.encode("utf-8")).hexdigest() != lock["mixtures_sha256"]:
        fail(f"assertion 4: {MIXTURES_DIR} differs from the lock's mixtures_sha256 (scripts/fetch-coolprop.sh)")
    read = lambda name: json.loads((MIXTURES_DIR / name).read_text(encoding="utf-8"))
    return read("mixture_binary_pairs.json"), read("predefined_mixtures.json")


def assert_pair(CP, pair):
    """Assertion 4 for a binary pair: the oracle's loaded parameters are the repo file's. CoolProp keys a pair by its CAS
    numbers sorted as strings and inverts beta where that swaps the file's order (MixtureParameters.cpp:130-163)."""
    keys = ["betaT", "gammaT", "betaV", "gammaV", "F"] if "betaT" in pair else ["xi", "zeta", "F"]
    cas1, cas2 = sorted((pair["CAS1"], pair["CAS2"]))
    swapped = cas1 != pair["CAS1"]
    for key in keys:
        loaded = float(CP.get_mixture_binary_pair_data(cas1, cas2, key))
        want = 1.0 / pair[key] if swapped and key in ("betaT", "betaV") else pair[key]
        if abs(loaded - want) > 1e-15 * abs(want):
            fail(f"assertion 4: CoolProp's {key} of {pair['Name1']}&{pair['Name2']} is {loaded}, the file's {want}")


def assert_fluid(CP, files, fluid):
    """Assertion 4: the oracle's data is the repo data; returns the file's sha256 for the header."""
    path = files.get(fluid)
    if path is None:
        fail(f"assertion 4: no file for {fluid} in {FLUIDS_DIR}")
    if json.loads(CP.get_fluid_param_string(fluid, "JSON"))[0] != json.loads(path.read_text(encoding="utf-8")):
        fail(f"assertion 4: CoolProp's JSON for {fluid} differs from {path.name}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def splitmix64(seed):
    mask = (1 << 64) - 1
    state = seed
    while True:
        state = (state + 0x9E3779B97F4A7C15) & mask
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & mask
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & mask
        yield z ^ (z >> 31)


def assert_golden():
    """Assertion 5: the sampler is the one phasekit_verify::sample implements."""
    rng = splitmix64(1)
    if [next(rng) for _ in GOLDEN] != GOLDEN:
        fail("assertion 5: SplitMix64 seed 1 does not give the golden vector")


def canonical_json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def error_class(exception):
    message = str(exception).lower()
    for name, needles in ERROR_CLASSES:
        if any(needle in message for needle in needles):
            return name
    return "other"


def abstract_state(CP, backend, fluids, *args):
    """`AbstractState(backend;fluids;[pair;v1;v2;]method;args...)`: a state updated with `pair` when one is given, then
    one method call; a string argument names a CoolProp constant (`iT`, `iP`)."""
    state = CP.AbstractState(backend, fluids)
    if args[0].endswith("_INPUTS"):
        pair, v1, v2, *args = args
        state.update(getattr(CP, pair), v1, v2)
    method, *rest = args
    return getattr(state, method)(*(getattr(CP, a) if isinstance(a, str) else a for a in rest))


def vle_props(CP, *args):
    """PropsSI with superancillaries off, so a saturation input goes through CoolProp's own VLE; the switch is
    restored."""
    before = CP.get_config_bool(CP.ENABLE_SUPERANCILLARIES)
    CP.set_config_bool(CP.ENABLE_SUPERANCILLARIES, False)
    try:
        return CP.PropsSI(*args)
    finally:
        CP.set_config_bool(CP.ENABLE_SUPERANCILLARIES, before)


def call(CP, fn, args):
    """One oracle call -> (value, status). `PropsSI` raises on failure, but `Props1SI` returns inf and leaves the
    message in the process-wide errstring (CoolProp's C convention; reading errstring clears it): both are checked."""
    CP.get_global_param_string("errstring")
    try:
        if fn == "AbstractState":
            value = float(abstract_state(CP, *args))
        elif fn == "PropsSIVle":
            value = float(vle_props(CP, *args))
        else:
            value = float(getattr(CP, fn)(*args))
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        return math.nan, f"err:{error_class(exception)}"
    message = CP.get_global_param_string("errstring")
    if message:
        return math.nan, f"err:{error_class(message)}"
    return value, "ok"


def cell(value):
    """Python repr, the shortest round trip; NaN and the infinities as nan, inf, -inf."""
    if math.isnan(value):
        return "nan"
    return repr(float(value))


def fnv1a64(values):
    """FNV-1a 64 over the little-endian bits of every float cell, NaN as 0x7ff8000000000000 (section 3.3)."""
    h = 0xCBF29CE484222325
    for value in values:
        data = struct.pack("<Q", 0x7FF8000000000000) if math.isnan(value) else struct.pack("<d", value)
        for byte in data:
            h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def header(kind, lock, config, fluids, columns, tol, floats, source="coolprop", units=None, grid=None):
    """The `<kind>/v1` header (section 3.3); `fluids` is (name, json sha256) pairs or one line naming them."""
    generator = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    libc = "-".join(part for part in platform.libc_ver() if part) or "unknown"
    lines = [
        f"fixture: {kind}/v1",
        f"oracle: CoolProp {lock['version']} git={lock['git']} so_sha256={lock['so_sha256']}",
        f"generator: gen.py sha256={generator} python={platform.python_version()} libc={libc}",
        f"config: {config}",
        "env: scrubbed COOLPROP_* PXFLASH_*; LC_ALL=C",
        "fluid: " + (fluids if isinstance(fluids, str) else " ".join(f"{n} json_sha256={sha}" for n, sha in fluids)),
        *([f"grid: {grid}"] if grid else []),
        f"source: {source}",
        "columns: " + ",".join(columns),
        *(["units: " + ",".join(units)] if units else []),
        "tol: " + ",".join(tol),
        f"bits: fnv1a64={fnv1a64(floats)}",
    ]
    return "".join(f"# {line}\n" for line in lines)


def fact_fluids(fn, inputs):
    """The pure fluids a fact reads: `PR::n-Propane` is n-Propane, `HEOS::A[0.4]&B[0.6]` is A and B."""
    name = {"PropsSI": inputs[-1], "PropsSIVle": inputs[-1], "Props1SI": inputs[0], "AbstractState": inputs[1]}[fn]
    return [part.split("[")[0] for part in name.split("::")[-1].split("&")]


def facts(CP, lock, config, files, args):
    rows = FACTS.get(args.set)
    if rows is None:
        fail(f"--set must be one of {', '.join(sorted(FACTS))} for --kind facts")
    fluids = sorted({fluid for _, fn, inputs in rows for fluid in fact_fluids(fn, inputs)})
    shas = [(fluid, assert_fluid(CP, files, fluid)) for fluid in fluids]
    lines, floats = [], []
    for name, fn, inputs in rows:
        value, status = call(CP, fn, inputs)
        floats.append(value)
        lines.append(f"{name},{fn}({';'.join(str(a) for a in inputs)}),{status},{cell(value)}\n")
    columns, tol = ["name", "call", "status", "value"], ["label", "label", "label", "exact"]
    text = header("facts", lock, config, shas, columns, tol, floats)
    return {f"facts/{args.set}.csv": text + "".join(lines)}


# `EOS[0].SUPERANCILLARY.check_points` keys: T, the multiprecision p, rho', rho'' and the SA/mp ratios.
CHECK_POINT_KEYS = [
    "T / K",
    "p(mp) / Pa",
    "rho'(mp) / mol/m^3",
    "rho''(mp) / mol/m^3",
    "p(SA)/p(mp)",
    "rho'(SA)/rho'(mp)",
    "rho''(SA)/rho''(mp)",
]


def checkpoints(CP, lock, config, files, args):
    """The superancillary check points of every fluid file that has a superancillary, sorted by name (section 3.5):
    3 per fluid, picked from fastchebpure's dense grid at Theta = (Tc - T)/Tc = 0.5, 0.3, 0.1 against the
    superancillary's numerical Tc (written beside them; a point below the triple point moves up to it), with the SA/mp
    ratios that set each point's `sa_fit` bound (section 5). Assertion 4 holds for every fluid copied."""
    lines, floats = [], []
    names = sorted(n for n, path in files.items() if "SUPERANCILLARY" in json.loads(path.read_text())["EOS"][0])
    for name in names:
        assert_fluid(CP, files, name)
        superancillary = json.loads(files[name].read_text(encoding="utf-8"))["EOS"][0]["SUPERANCILLARY"]
        tc = float(superancillary["meta"]["Tcrittrue / K"])
        for point in superancillary["check_points"]:
            values = [tc, *(float(point[key]) for key in CHECK_POINT_KEYS)]
            floats.extend(values)
            lines.append(",".join([name, *map(cell, values)]) + "\n")
    columns = ["fluid", "Tc", "T", "p", "rhoL", "rhoV", "p_sa_mp", "rhoL_sa_mp", "rhoV_sa_mp"]
    tol = ["label", "in", "in", "sa_fit", "sa_fit", "sa_fit", "in", "in", "in"]
    units = ["-", "K", "K", "Pa", "mol/m3", "mol/m3", "-", "-", "-"]
    fluids = f"{len(names)} with a superancillary, fluids_sha256={lock['fluids_sha256']}"
    text = header("checkpoints", lock, config, fluids, columns, tol, floats, "mp:coolprop-json", units)
    return {"mp/check-points.csv": text + "".join(lines)}


def tier_fluids(args, files):
    """The fluids of --tier (core: the committed subset; all and full: every fluid), narrowed by --fluids."""
    names = CORE_FLUIDS if args.tier == "core" else sorted(files)
    if args.fluids:
        wanted = args.fluids.split(",")
        unknown = [name for name in wanted if name not in names]
        if unknown:
            fail(f"--fluids: not in the {args.tier} tier: {', '.join(unknown)}")
        names = [name for name in names if name in wanted]
    return names


CRIT_COLUMNS = ["Tc_pub", "pc_pub", "rhoc_pub", "Tc_num", "pc_num", "rhoc_num", "Ttriple", "ptriple", "Tmin", "Tmax",
                "pmax", "M", "R"]
CRIT_UNITS = ["K", "Pa", "mol/m3", "K", "Pa", "mol/m3", "K", "Pa", "K", "K", "Pa", "kg/mol", "J/mol/K"]
CRIT_TOL = ["exact"] * 3 + ["flash"] * 3 + ["exact"] * 7


def crit_row(CP, lock, fluid):
    """One fluid's constants (section 3.5): the critical point with superancillaries off (the published one, JSON
    STATES.critical) and on (the superancillary's exact one; a pseudo-pure fluid has none and repeats the published
    point), then CoolProp's Ttriple and ptriple (the saturation minimum, map 09 R8), Tmin, Tmax, pmax, M and R. The
    switch is restored to the lock's value after each use."""
    row = []
    for enabled in (False, True):
        CP.set_config_bool(CP.ENABLE_SUPERANCILLARIES, enabled)
        state = CP.AbstractState("HEOS", fluid)
        row += [state.T_critical(), state.p_critical(), state.rhomolar_critical()]
    CP.set_config_bool(CP.ENABLE_SUPERANCILLARIES, json.loads(lock["config_json"])["ENABLE_SUPERANCILLARIES"])
    state = CP.AbstractState("HEOS", fluid)
    return row + [state.Ttriple(), state.p_triple(), state.Tmin(), state.Tmax(), state.pmax(), state.molar_mass(),
                  state.gas_constant()]


def crit(CP, lock, config, files, args):
    """The `crit` kind (section 3.5), one row per fluid: crit/<Fluid>.csv in the core tier, all/crit.csv (with a
    fluid column, sorted by name) otherwise. Assertion 4 holds for every fluid. A failed call stops the generator: every
    fluid has these constants."""
    names = tier_fluids(args, files)
    rows = {}
    for name in names:
        sha = assert_fluid(CP, files, name)
        try:
            rows[name] = (sha, crit_row(CP, lock, name))
        except Exception as exception:  # a constant every fluid has; a failure is a generator bug
            fail(f"crit: {name}: {exception}")
    if args.tier == "core":
        out = {}
        for name, (sha, row) in rows.items():
            text = header("crit", lock, config, [(name, sha)], CRIT_COLUMNS, CRIT_TOL, row, units=CRIT_UNITS)
            out[f"crit/{name}.csv"] = text + ",".join(map(cell, row)) + "\n"
        return out
    floats = [value for _, row in rows.values() for value in row]
    fluids = f"{len(names)} fluids, fluids_sha256={lock['fluids_sha256']}"
    columns, tol, units = ["fluid", *CRIT_COLUMNS], ["label", *CRIT_TOL], ["-", *CRIT_UNITS]
    text = header("crit", lock, config, fluids, columns, tol, floats, units=units)
    lines = [",".join([name, *map(cell, row)]) + "\n" for name, (_, row) in rows.items()]
    return {"all/crit.csv": text + "".join(lines)}


# The `term` kind (section 3.5): CoolProp's alphar and its 14 derivatives to order 4, in the method order of
# `AbstractState`; each column is lower-cased in the file.
TERM_METHODS = ["alphar", "dalphar_dTau", "dalphar_dDelta", "d2alphar_dTau2", "d2alphar_dDelta_dTau", "d2alphar_dDelta2",
                "d3alphar_dTau3", "d3alphar_dDelta_dTau2", "d3alphar_dDelta2_dTau", "d3alphar_dDelta3", "d4alphar_dTau4",
                "d4alphar_dDelta_dTau3", "d4alphar_dDelta2_dTau2", "d4alphar_dDelta3_dTau", "d4alphar_dDelta4"]
# The alpha^0 methods to order 3, the oracle's limit (map 10 section 8.2), in the columns of the first 10 alpha^r ones.
ALPHA0_METHODS = ["alpha0", "dalpha0_dTau", "dalpha0_dDelta", "d2alpha0_dTau2", "d2alpha0_dDelta_dTau", "d2alpha0_dDelta2",
                  "d3alpha0_dTau3", "d3alpha0_dDelta_dTau2", "d3alpha0_dDelta2_dTau", "d3alpha0_dDelta3"]
TERM_COLUMNS = ["block_idx", "block_type", "terms", "T", "rhomolar", "tau", "delta", "status",
                *(m.lower() for m in TERM_METHODS)]
TERM_UNITS = ["-", "-", "-", "K", "mol/m3", "-", "-", "-", *["-"] * len(TERM_METHODS)]
TERM_TOL = ["label", "label", "in", "in", "in", "in", "in", "label", *["term"] * len(TERM_METHODS)]
TERM_DELTA_MIN = 1e-8
TERM_ROWS = {"core": 100, "all": 0, "full": 300}  # per block; every block of a fluid shares the same (tau, delta) points
TERM_TOTALS = {"core": 0, "all": 4, "full": 64}  # alpha^r total rows per fluid, on the first points of the same grid
TERM_NEAR_CRITICAL = 20  # extra NonAnalytic rows with |tau - 1| and |delta - 1| ~ logU[1e-6, 1e-2]
TERM_PREFIX = "ResidualHelmholtz"


def uniform(rng, lo, hi):
    """`phasekit_verify::sample::SplitMix64::uniform`: the top 53 bits times 2^-53, then lo + (hi - lo) * u."""
    return lo + (hi - lo) * ((next(rng) >> 11) * 2.0**-53)


def log_uniform(rng, lo, hi):
    """`SplitMix64::log_uniform`: uniform in ln, clamped to [lo, hi]."""
    return min(max(math.exp(uniform(rng, math.log(lo), math.log(hi))), lo), hi)


def term_grid(seed, rows, tau_range, delta_max):
    """The shared (tau, delta) points of one fluid and the NonAnalytic neighbourhood of tau = delta = 1 (section 3.5),
    from one SplitMix64 stream per fluid, so a file does not depend on which other fluids are generated."""
    rng = splitmix64(seed)
    shared = [(uniform(rng, *tau_range), log_uniform(rng, TERM_DELTA_MIN, delta_max)) for _ in range(rows)]
    near = []
    for _ in range(TERM_NEAR_CRITICAL):
        tau, delta = (1.0 + (1 if next(rng) >> 63 else -1) * log_uniform(rng, 1e-6, 1e-2) for _ in range(2))
        near.append((tau, delta))
    return shared, near


def term_row(CP, fluid, index, kind, terms, t, rho):
    """One row: a fresh `AbstractState` of `fluid` with the phase imposed at (T, rho); (floats, line). `ideal` rows
    hold alpha^0 to order 3 and nan in the order-4 columns."""
    methods = ALPHA0_METHODS if kind == "ideal" else TERM_METHODS
    padding = [math.nan] * (len(TERM_METHODS) - len(methods))
    try:
        state = CP.AbstractState("HEOS", fluid)
        state.specify_phase(CP.iphase_gas)
        state.update(CP.DmolarT_INPUTS, rho, t)
        values, status = [state.tau(), state.delta(), *(getattr(state, m)() for m in methods), *padding], "ok"
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        values, status = [math.nan] * (2 + len(TERM_METHODS)), f"err:{error_class(exception)}"
    row = [float(terms), t, rho, *values]
    return row, ",".join([index, kind, *map(cell, row[:5]), status, *map(cell, row[5:])]) + "\n"


def term_rows(job):
    """One fluid's rows, in a pool child (map 10 section 8.2): each residual block becomes a renamed one-block clone
    loaded with `add_fluids_as_JSON` (EOS[0] only, no superancillary), and every row is a fresh `AbstractState` with
    the phase imposed, evaluated at T = T_r/tau, rho = delta*rho_r; tau and delta are the ones CoolProp then holds.
    The totals rows (`block_idx = all`) evaluate the fluid itself."""
    import CoolProp.CoolProp as CP  # the parent's module, inherited through fork

    name, fluid, points = job
    lines, floats = [], []
    blocks = fluid["EOS"][0]["alphar"]
    for index, block in enumerate(blocks if points["shared"] else []):
        clone = json.loads(json.dumps(fluid))
        alias = f"PHASEKIT_TERM_{index}_{name}"
        clone["INFO"].update(NAME=alias, ALIASES=[], CAS=alias, REFPROP_NAME=alias)
        clone["EOS"] = [clone["EOS"][0]]
        clone["EOS"][0]["alphar"] = [block]
        clone["EOS"][0].pop("SUPERANCILLARY", None)
        CP.add_fluids_as_JSON("HEOS", json.dumps([clone]))
        kind = block["type"].removeprefix(TERM_PREFIX)
        grid = points["shared"] + (points["near"] if kind == "NonAnalytic" else [])
        for t, rho in grid:
            row, line = term_row(CP, alias, str(index), kind, len(block["n"]), t, rho)
            floats.extend(row)
            lines.append(line)
    terms = sum(len(block["n"]) for block in blocks)
    for t, rho in points["totals"]:
        row, line = term_row(CP, name, "all", "all", terms, t, rho)
        floats.extend(row)
        lines.append(line)
    for t, rho in points["totals"]:
        row, line = term_row(CP, name, "ideal", "ideal", len(fluid["EOS"][0]["alpha0"]), t, rho)
        floats.extend(row)
        lines.append(line)
    return lines, floats


def pool_map(function, jobs, workers):
    """`function` over `jobs` in a fork pool (children inherit the loaded library; section 3.1), results in job order,
    so bytes do not depend on --jobs. Children are used even for one worker: they may mutate the library."""
    import multiprocessing

    with multiprocessing.get_context("fork").Pool(workers or os.cpu_count()) as pool:
        return pool.map(function, jobs, chunksize=1)


def term(CP, lock, config, files, args):
    """The `term` kind (section 3.5): the oracle's alphar and 14 derivatives on one grid per fluid (tau ~ U[T_r/Tmax,
    T_r/Tmin], delta ~ logU[1e-8, rho_max/rho_r] with rho_max the saturated liquid at the minimum temperature, the
    record's density bound). The core tier has 100 points per block of every core fluid (NonAnalytic blocks also 20
    near tau = delta = 1) in term/<Fluid>.csv; the all-fluid tier the alpha^r totals (`block_idx = all`) and the alpha^0
    rows (`block_idx = ideal`, orders 0-3, `terms` the number of alpha^0 blocks) at the first 4 points of every fluid in
    all/term.csv; the full tier 300 points per block and 64 of each kind of total per fluid."""
    rows, totals = args.rows or TERM_ROWS[args.tier], TERM_TOTALS[args.tier]
    jobs, shas = [], {}
    for name in tier_fluids(args, files):
        shas[name] = assert_fluid(CP, files, name)
        fluid = json.loads(files[name].read_text(encoding="utf-8"))
        eos = fluid["EOS"][0]
        state = CP.AbstractState("HEOS", name)
        t_r, rho_r = state.T_reducing(), state.rhomolar_reducing()
        tau_range = (t_r / state.Tmax(), t_r / state.Tmin())
        delta_max = eos["STATES"]["sat_min_liquid"]["rhomolar"] / rho_r
        shared, near = term_grid(args.seed, max(rows, totals), tau_range, delta_max)
        to_t_rho = lambda grid: [(t_r / tau, delta * rho_r) for tau, delta in grid]  # noqa: E731
        points = {"shared": to_t_rho(shared[:rows]), "near": to_t_rho(near), "totals": to_t_rho(shared[:totals])}
        jobs.append((name, fluid, points))
    grid = (f"tau~U[Tr/Tmax,Tr/Tmin] delta~logU[{TERM_DELTA_MIN!r},rhomax/rhor] n={rows} seed={args.seed} "
            f"phase=imposed:gas; NonAnalytic +{TERM_NEAR_CRITICAL} at |tau-1|,|delta-1|~logU[1e-6,1e-2]; "
            f"totals n={totals}")
    results = pool_map(term_rows, jobs, args.jobs)
    if args.tier == "all":
        floats = [value for _, values in results for value in values]
        fluids = f"{len(jobs)} fluids, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *TERM_COLUMNS], ["label", *TERM_TOL], ["-", *TERM_UNITS]
        text = header("term", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        lines = [f"{name},{line}" for (name, _, _), (rows_, _) in zip(jobs, results) for line in rows_]
        return {"all/term.csv": text + "".join(lines)}
    out = {}
    for (name, _, _), (lines, floats) in zip(jobs, results):
        text = header("term", lock, config, [(name, shas[name])], TERM_COLUMNS, TERM_TOL, floats, units=TERM_UNITS,
                      grid=grid)
        out[f"term/{name}.csv"] = text + "".join(lines)
    return out


# The `eos` kind (section 3.5): properties at (T, rho) with the phase imposed, molar SI. Each output is one
# `AbstractState` call; `speed_sound` is nan where (dp/drho)_T < 0 (map 10 R18), which is not a failure.
EOS_OUTPUTS = [
    ("p", "Pa", lambda CP, s: s.p()),
    ("hmolar", "J/mol", lambda CP, s: s.hmolar()),
    ("smolar", "J/mol/K", lambda CP, s: s.smolar()),
    ("umolar", "J/mol", lambda CP, s: s.umolar()),
    ("cvmolar", "J/mol/K", lambda CP, s: s.cvmolar()),
    ("cpmolar", "J/mol/K", lambda CP, s: s.cpmolar()),
    ("speed_sound", "m/s", lambda CP, s: s.speed_sound()),
    ("Z", "-", lambda CP, s: s.compressibility_factor()),
    ("dpdrho_T", "Pa*m3/mol", lambda CP, s: s.first_partial_deriv(CP.iP, CP.iDmolar, CP.iT)),
    ("dpdT_rho", "Pa/K", lambda CP, s: s.first_partial_deriv(CP.iP, CP.iT, CP.iDmolar)),
    ("Bvirial", "m3/mol", lambda CP, s: s.Bvirial()),
    ("Cvirial", "m6/mol2", lambda CP, s: s.Cvirial()),
    ("dBvirial_dT", "m3/mol/K", lambda CP, s: s.dBvirial_dT()),
    ("dCvirial_dT", "m6/mol2/K", lambda CP, s: s.dCvirial_dT()),
    ("cp0molar", "J/mol/K", lambda CP, s: s.cp0molar()),
    ("hmolar_residual", "J/mol", lambda CP, s: s.hmolar_residual()),
    ("smolar_residual", "J/mol/K", lambda CP, s: s.smolar_residual()),
    ("gmolar_residual", "J/mol", lambda CP, s: s.gibbsmolar_residual()),
]
# Since M5.5, 12 first partials (∂of/∂wrt)_at: of = v[i], wrt = v[i + 2], at = v[i + 5] (mod 12) over CoolProp's 12
# first-order variables, so each is once differentiated, once the variable and once held constant, and no partial
# holds the quantity it differentiates by (map 01 section 4a).
EOS_PARTIAL_VARS = ["T", "P", "Dmolar", "Dmass", "Hmolar", "Hmass", "Smolar", "Smass", "Umolar", "Umass", "Gmolar",
                    "Gmass"]
EOS_PARTIALS = [(EOS_PARTIAL_VARS[i], EOS_PARTIAL_VARS[(i + 2) % 12], EOS_PARTIAL_VARS[(i + 5) % 12]) for i in range(12)]
EOS_OUTPUTS += [
    (f"d{of}_d{wrt}_{at}", "-",
     lambda CP, s, of=of, wrt=wrt, at=at: s.first_partial_deriv(*(getattr(CP, f"i{v}") for v in (of, wrt, at))))
    for of, wrt, at in EOS_PARTIALS
]
# Since M7.10, 10 second partials (∂/∂wrt2 (∂of/∂wrt1)_at1)_at2 over the 10 variables CoolProp differentiates twice (its
# `get_dT_drho_second_derivatives` refuses g): of = v[i], wrt1 = v[i + 2], at1 = v[i + 5], wrt2 = v[i + 3], at2 =
# v[i + 6] (mod 10), so no partial holds the quantity it differentiates by, in either basis; then the fundamental
# derivative of gas dynamics, the isothermal compressibility and the isobaric expansion coefficient.
EOS_SECOND_VARS = ["T", "P", "Dmolar", "Dmass", "Hmolar", "Hmass", "Smolar", "Smass", "Umolar", "Umass"]
EOS_SECONDS = [tuple(EOS_SECOND_VARS[(i + k) % 10] for k in (0, 2, 5, 3, 6)) for i in range(10)]
EOS_OUTPUTS += [
    ("d2{}_d{}_{}_d{}_{}".format(*vs), "-",
     lambda CP, s, vs=vs: s.second_partial_deriv(*(getattr(CP, f"i{v}") for v in vs)))
    for vs in EOS_SECONDS
]
EOS_OUTPUTS += [
    ("fundamental_derivative", "-", lambda CP, s: s.fundamental_derivative_of_gas_dynamics()),
    ("isothermal_compressibility", "1/Pa", lambda CP, s: s.isothermal_compressibility()),
    ("isobaric_expansion", "1/K", lambda CP, s: s.isobaric_expansion_coefficient()),
]
EOS_COLUMNS = ["T", "rhomolar", "region", "status", *(name for name, _, _ in EOS_OUTPUTS)]
EOS_UNITS = ["K", "mol/m3", "-", "-", *(unit for _, unit, _ in EOS_OUTPUTS)]
EOS_TOL = ["in", "in", "label", "label", *["prop"] * len(EOS_OUTPUTS)]
EOS_ROWS = {"core": 500, "all": 8, "full": 10000}
EOS_RHO_SPAN = 1e-6  # rho ~ logU[EOS_RHO_SPAN * rhoL(T_low), rhoL(T_low)]


def saturated_density(CP, name, q, t):
    """rho' (q = 0) or rho'' (q = 1) at T from QT, or from the fluid's saturated-density ancillary where CoolProp's QT
    solver finds no root: a pseudo-pure fluid within about 2 K of Tc (R410A, R507A, SES36; measured at M5.1)."""
    state = CP.AbstractState("HEOS", name)
    try:
        state.update(CP.QT_INPUTS, q, t)
        return state.rhomolar()
    except Exception:  # only the region label needs it; the ancillary never fails below Tc
        return state.saturation_ancillary(CP.iDmolar, q, CP.iT, t)


def eos_region(CP, name, t, rho, tc):
    """`stable` above the critical temperature and outside the saturated densities at T (bubble and dew of a
    pseudo-pure fluid), else `metastable` where (dp/drho)_T > 0 and `unstable` where it is not (map 10 section 8.5)."""
    if t >= tc:
        return "stable"
    if rho >= saturated_density(CP, name, 0, t) or rho <= saturated_density(CP, name, 1, t):
        return "stable"
    state = CP.AbstractState("HEOS", name)
    state.specify_phase(CP.iphase_gas)
    state.update(CP.DmolarT_INPUTS, rho, t)
    return "metastable" if state.first_partial_deriv(CP.iP, CP.iDmolar, CP.iT) > 0 else "unstable"


def eos_rows(job):
    """One fluid's rows, in a pool child: each a fresh `AbstractState` with the phase imposed (gas) at (T, rho). An
    exception in any output makes the row `err:<class>` with nan outputs; a nan output in an `ok` row is the oracle's."""
    import CoolProp.CoolProp as CP  # the parent's module, inherited through fork

    name, tc, points = job
    lines, floats = [], []
    for t, rho in points:
        try:
            region = eos_region(CP, name, t, rho, tc)
        except Exception as exception:  # the region needs the saturated densities, which every fluid has below Tc
            raise RuntimeError(f"eos: {name}: no region at T = {t!r}, rho = {rho!r}: {exception}") from None
        try:
            state = CP.AbstractState("HEOS", name)
            state.specify_phase(CP.iphase_gas)
            state.update(CP.DmolarT_INPUTS, rho, t)
            values, status = [get(CP, state) for _, _, get in EOS_OUTPUTS], "ok"
        except Exception as exception:  # every oracle failure becomes a status, never a crash
            values, status = [math.nan] * len(EOS_OUTPUTS), f"err:{error_class(exception)}"
        row = [t, rho, *values]
        floats.extend(row)
        lines.append(",".join([cell(t), cell(rho), region, status, *map(cell, values)]) + "\n")
    return lines, floats


def eos(CP, lock, config, files, args):
    """The `eos` kind (section 3.5): T ~ U[T_low, Tmax] and rho ~ logU[1e-6 rhoL(T_low), rhoL(T_low)] with T_low =
    max(Tmin, Ttriple) (map 09 R8) and rhoL(T_low) the saturated liquid there, from one SplitMix64 stream per fluid (T
    then rho per row), so the all-fluid tier's rows are the first of the core grid. 500 rows per core fluid in
    eos/<Fluid>.csv, 8 per fluid in all/eos.csv, 10,000 per fluid in the full set."""
    rows = args.rows or EOS_ROWS[args.tier]
    jobs, shas = [], {}
    for name in tier_fluids(args, files):
        shas[name] = assert_fluid(CP, files, name)
        state = CP.AbstractState("HEOS", name)
        t_low, t_max, tc = max(state.Tmin(), state.Ttriple()), state.Tmax(), state.T_critical()
        state.update(CP.QT_INPUTS, 0, t_low)
        rho_l = state.rhomolar()
        rng = splitmix64(args.seed)
        points = []
        for _ in range(rows):
            t = uniform(rng, t_low, t_max)
            points.append((t, log_uniform(rng, EOS_RHO_SPAN * rho_l, rho_l)))
        jobs.append((name, tc, points))
    grid = (f"T~U[Tlow,Tmax] rho~logU[{EOS_RHO_SPAN!r}*rhoL(Tlow),rhoL(Tlow)] Tlow=max(Tmin,Ttriple) n={rows} "
            f"seed={args.seed} phase=imposed:gas")
    try:
        results = pool_map(eos_rows, jobs, args.jobs)
    except RuntimeError as exception:  # a generator bug, not an oracle failure
        fail(str(exception))
    if args.tier == "all":
        floats = [value for _, values in results for value in values]
        fluids = f"{len(jobs)} fluids, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *EOS_COLUMNS], ["label", *EOS_TOL], ["-", *EOS_UNITS]
        text = header("eos", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        lines = [f"{name},{line}" for (name, _, _), (rows_, _) in zip(jobs, results) for line in rows_]
        return {"all/eos.csv": text + "".join(lines)}
    out = {}
    for (name, _, _), (lines, floats) in zip(jobs, results):
        text = header("eos", lock, config, [(name, shas[name])], EOS_COLUMNS, EOS_TOL, floats, units=EOS_UNITS,
                      grid=grid)
        out[f"eos/{name}.csv"] = text + "".join(lines)
    return out


# The `cubic` kind (PLAN.md M11.4; map 06 section 8): CoolProp 8.0.0's SRK and PR, at c = 0, for every fluid of the tier
# its cubic library knows, at 8 supercritical states of each cubic: T in {1.05, 1.5} T_c and rho in {0.05, 0.5, 1, 1.8}
# rho_c, with T_c and p_c the library's and rho_c = p_c/(Z_c R T_c) the cubic's exact critical density. The outputs the
# oracle can give at c = 0 (map 06 section 8): alpha^r and its A_ij to order 2, p, h, u, g, a, c_v, c_p, w and c_p0;
# never entropy (DIV-0010).
CUBIC_KINDS = (("SRK", 1.0 / 3.0), ("PR", 0.30740130869870386))
CUBIC_OUTPUTS = [
    ("alphar", "-", lambda s: s.alphar()),
    ("A10", "-", lambda s: s.tau() * s.dalphar_dTau()),
    ("A01", "-", lambda s: s.delta() * s.dalphar_dDelta()),
    ("A20", "-", lambda s: s.tau() ** 2 * s.d2alphar_dTau2()),
    ("A11", "-", lambda s: s.tau() * s.delta() * s.d2alphar_dDelta_dTau()),
    ("A02", "-", lambda s: s.delta() ** 2 * s.d2alphar_dDelta2()),
    ("p", "Pa", lambda s: s.p()),
    ("hmolar", "J/mol", lambda s: s.hmolar()),
    ("umolar", "J/mol", lambda s: s.umolar()),
    ("gibbsmolar", "J/mol", lambda s: s.gibbsmolar()),
    ("helmholtzmolar", "J/mol", lambda s: s.helmholtzmolar()),
    ("cvmolar", "J/mol/K", lambda s: s.cvmolar()),
    ("cpmolar", "J/mol/K", lambda s: s.cpmolar()),
    ("speed_sound", "m/s", lambda s: s.speed_sound()),
    ("cp0molar", "J/mol/K", lambda s: s.cp0molar()),
]
CUBIC_COLUMNS = ["fluid", "kind", "T", "rhomolar", "status", *(name for name, _, _ in CUBIC_OUTPUTS)]
CUBIC_UNITS = ["-", "-", "K", "mol/m3", "-", *(unit for _, unit, _ in CUBIC_OUTPUTS)]
CUBIC_TOL = ["label", "label", "in", "in", "label", *["prop"] * len(CUBIC_OUTPUTS)]


def cubic(CP, lock, config, files, args):
    """The `cubic` kind: all/cubic.csv over the tier's fluids that v8.0.0's cubic library has (116 with --tier all)."""
    lines, floats, count = [], [], 0
    for name in tier_fluids(args, files):
        try:
            CP.AbstractState("PR", name)
        except Exception:  # not in the cubic library (map 06 C5: 14 HEOS fluids and the 6 blends)
            continue
        assert_fluid(CP, files, name)
        count += 1
        for kind, z_c in CUBIC_KINDS:
            probe = CP.AbstractState(kind, name)
            tc, pc = probe.T_critical(), probe.p_critical()
            rhoc = pc / (z_c * 8.31446261815324 * tc)
            for t in (1.05 * tc, 1.5 * tc):
                for rho in (0.05 * rhoc, 0.5 * rhoc, rhoc, 1.8 * rhoc):
                    try:
                        state = CP.AbstractState(kind, name)
                        state.update(CP.DmolarT_INPUTS, rho, t)
                        values, status = [get(state) for _, _, get in CUBIC_OUTPUTS], "ok"
                    except Exception as exception:  # every oracle failure becomes a status, never a crash
                        values, status = [math.nan] * len(CUBIC_OUTPUTS), f"err:{error_class(exception)}"
                    floats.extend([t, rho, *values])
                    lines.append(",".join([name, kind, cell(t), cell(rho), status, *map(cell, values)]) + "\n")
    fluids = f"{count} fluids, fluids_sha256={lock['fluids_sha256']}"
    grid = "T in {1.05,1.5}*Tc; rho in {0.05,0.5,1,1.8}*pc/(Zc*R*Tc); Tc, pc the cubic library's"
    text = header("cubic", lock, config, fluids, CUBIC_COLUMNS, CUBIC_TOL, floats, units=CUBIC_UNITS, grid=grid)
    return {"all/cubic.csv": text + "".join(lines)}


# The `cubic_sat` kind (PLAN.md M11.5; map 06 U4): v8.0.0's cubic superancillary (Bell and Deiters 2021) through
# `saturation_ancillary`, p, rho' and rho'' of SRK and PR at T in {0.3, 0.5, 0.7, 0.9, 0.99} T_c (the library's T_c),
# for every fluid of the tier its cubic library knows; a T below the curve's range is an `err` row.
CUBIC_SAT_COLUMNS = ["fluid", "kind", "T", "status", "p", "rhoL", "rhoV"]
CUBIC_SAT_UNITS = ["-", "-", "K", "-", "Pa", "mol/m3", "mol/m3"]
CUBIC_SAT_TOL = ["label", "label", "in", "label", "prop", "prop", "prop"]


def cubic_sat(CP, lock, config, files, args):
    """The `cubic_sat` kind: all/cubic_sat.csv over the tier's fluids that v8.0.0's cubic library has."""
    lines, floats, count = [], [], 0
    for name in tier_fluids(args, files):
        try:
            CP.AbstractState("PR", name)
        except Exception:  # not in the cubic library
            continue
        assert_fluid(CP, files, name)
        count += 1
        for kind, _ in CUBIC_KINDS:
            tc = CP.AbstractState(kind, name).T_critical()
            for t in (0.3 * tc, 0.5 * tc, 0.7 * tc, 0.9 * tc, 0.99 * tc):
                try:
                    state = CP.AbstractState(kind, name)
                    values = [state.saturation_ancillary(CP.iP, 0, CP.iT, t),
                              state.saturation_ancillary(CP.iDmolar, 0, CP.iT, t),
                              state.saturation_ancillary(CP.iDmolar, 1, CP.iT, t)]
                    status = "ok"
                except Exception as exception:  # below the superancillary's range
                    values, status = [math.nan] * 3, f"err:{error_class(exception)}"
                floats.extend([t, *values])
                lines.append(",".join([name, kind, cell(t), status, *map(cell, values)]) + "\n")
    fluids = f"{count} fluids, fluids_sha256={lock['fluids_sha256']}"
    grid = "T in {0.3,0.5,0.7,0.9,0.99}*Tc, Tc the cubic library's"
    text = header("cubic_sat", lock, config, fluids, CUBIC_SAT_COLUMNS, CUBIC_SAT_TOL, floats, units=CUBIC_SAT_UNITS,
                  grid=grid)
    return {"all/cubic_sat.csv": text + "".join(lines)}


# The `mix_alphar` kind (PLAN.md M13.3): CoolProp 8.0.0's multi-fluid residual, the gas phase imposed so that (T, rho)
# is evaluated as given, for every binary pair of its library at z1 = 0.4 (the file's first component; Bell et al.'s
# check tables use it) and for every predefined mixture it can build, at two states of each mixture's own reducing
# state: (tau, delta) = (0.8, 0.8) and (1.25, 1.6). Columns: the reducing T and rho, alpha^r and its A_ij to order 3.
MIX_ALPHAR_OUTPUTS = [
    ("Tr", "K", lambda s: s.T_reducing()),
    ("rhor", "mol/m3", lambda s: s.rhomolar_reducing()),
    ("alphar", "-", lambda s: s.alphar()),
    ("A10", "-", lambda s: s.tau() * s.dalphar_dTau()),
    ("A01", "-", lambda s: s.delta() * s.dalphar_dDelta()),
    ("A20", "-", lambda s: s.tau() ** 2 * s.d2alphar_dTau2()),
    ("A11", "-", lambda s: s.tau() * s.delta() * s.d2alphar_dDelta_dTau()),
    ("A02", "-", lambda s: s.delta() ** 2 * s.d2alphar_dDelta2()),
    ("A30", "-", lambda s: s.tau() ** 3 * s.d3alphar_dTau3()),
    ("A21", "-", lambda s: s.tau() ** 2 * s.delta() * s.d3alphar_dDelta_dTau2()),
    ("A12", "-", lambda s: s.tau() * s.delta() ** 2 * s.d3alphar_dDelta2_dTau()),
    ("A03", "-", lambda s: s.delta() ** 3 * s.d3alphar_dDelta3()),
]
MIX_ALPHAR_STATES = ((0.8, 0.8), (1.25, 1.6))
MIX_ALPHAR_COLUMNS = ["mixture", "z1", "T", "rhomolar", "status", *(name for name, _, _ in MIX_ALPHAR_OUTPUTS)]
MIX_ALPHAR_UNITS = ["-", "-", "K", "mol/m3", "-", *(unit for _, unit, _ in MIX_ALPHAR_OUTPUTS)]
MIX_ALPHAR_TOL = ["label", "in", "in", "in", "label", *["prop"] * len(MIX_ALPHAR_OUTPUTS)]


def mix_alphar_rows(CP, label, z1, fluids, fractions, floats, lines):
    """The rows of one mixture: an `err` row per state where the oracle cannot build or evaluate it."""
    for tau, delta in MIX_ALPHAR_STATES:
        t = rho = math.nan
        try:
            state = CP.AbstractState("HEOS", fluids)
            if fractions is not None:
                state.set_mole_fractions(fractions)
            t, rho = state.T_reducing() / tau, delta * state.rhomolar_reducing()
            state.specify_phase(CP.iphase_gas)
            state.update(CP.DmolarT_INPUTS, rho, t)
            values, status = [get(state) for _, _, get in MIX_ALPHAR_OUTPUTS], "ok"
        except Exception as exception:  # a missing pair, an unknown component: a status, never a crash
            values, status = [math.nan] * len(MIX_ALPHAR_OUTPUTS), f"err:{error_class(exception)}"
        floats.extend([z1, t, rho, *values])
        lines.append(",".join([label, cell(z1), cell(t), cell(rho), status, *map(cell, values)]) + "\n")


def mix_alphar(CP, lock, config, files, args):
    """The `mix_alphar` kind: all/mix_alphar.csv over the library's pairs and predefined mixtures (any --tier)."""
    pairs, predefined = mixture_library(lock)
    lines, floats, seen = [], [], set()
    for pair in sorted(pairs, key=lambda p: tuple(sorted((p["CAS1"], p["CAS2"])))):
        assert_pair(CP, pair)
        mix_alphar_rows(CP, f"{pair['Name1']}&{pair['Name2']}", 0.4, f"{pair['Name1']}&{pair['Name2']}", [0.4, 0.6],
                        floats, lines)
    for mixture in sorted(predefined, key=lambda m: m["name"]):
        if mixture["name"] in seen:
            continue
        seen.add(mixture["name"])
        mix_alphar_rows(CP, f"{mixture['name']}.mix", math.nan, f"{mixture['name']}.mix", None, floats, lines)
    fluids = (f"{len(pairs)} pairs and {len(seen)} predefined mixtures, fluids_sha256={lock['fluids_sha256']}, "
              f"mixtures_sha256={lock['mixtures_sha256']}")
    grid = "pairs at z1 = 0.4; (tau, delta) in {(0.8,0.8),(1.25,1.6)} of each mixture's reducing state; gas imposed"
    text = header("mix_alphar", lock, config, fluids, MIX_ALPHAR_COLUMNS, MIX_ALPHAR_TOL, floats,
                  units=MIX_ALPHAR_UNITS, grid=grid)
    return {"all/mix_alphar.csv": text + "".join(lines)}


# The `mix_eos` kind (PLAN.md M13.4): CoolProp 8.0.0's mixtures at fixed composition, the gas phase imposed so that
# (T, rho) is evaluated as given, over `mix_alphar`'s mixtures at two states of each one's reducing state: (tau, delta)
# = (0.8, 0.5) and (1.25, 2.0). Columns: p, h, s, u, c_v, c_p, w, Z, and the ideal part's alpha0, tau dalpha0/dtau and
# tau^2 d2alpha0/dtau2.
MIX_EOS_OUTPUTS = [
    ("p", "Pa", lambda s: s.p()),
    ("hmolar", "J/mol", lambda s: s.hmolar()),
    ("smolar", "J/mol/K", lambda s: s.smolar()),
    ("umolar", "J/mol", lambda s: s.umolar()),
    ("cvmolar", "J/mol/K", lambda s: s.cvmolar()),
    ("cpmolar", "J/mol/K", lambda s: s.cpmolar()),
    ("speed_sound", "m/s", lambda s: s.speed_sound()),
    ("Z", "-", lambda s: s.compressibility_factor()),
    ("alpha0", "-", lambda s: s.alpha0()),
    ("A0_10", "-", lambda s: s.tau() * s.dalpha0_dTau()),
    ("A0_20", "-", lambda s: s.tau() ** 2 * s.d2alpha0_dTau2()),
    # The first two components' (every pair's; M13.13 reads the predefined blends): M13.6.
    ("fugacity_coefficient_1", "-", lambda s: s.fugacity_coefficient(0)),
    ("fugacity_coefficient_2", "-", lambda s: s.fugacity_coefficient(1)),
    ("chemical_potential_1", "J/mol", lambda s: s.chemical_potential(0)),
    ("chemical_potential_2", "J/mol", lambda s: s.chemical_potential(1)),
]
MIX_EOS_STATES = ((0.8, 0.5), (1.25, 2.0))
MIX_EOS_COLUMNS = ["mixture", "z1", "T", "rhomolar", "status", *(name for name, _, _ in MIX_EOS_OUTPUTS)]
MIX_EOS_UNITS = ["-", "-", "K", "mol/m3", "-", *(unit for _, unit, _ in MIX_EOS_OUTPUTS)]
MIX_EOS_TOL = ["label", "in", "in", "in", "label", *["prop"] * len(MIX_EOS_OUTPUTS)]


def mix_eos_rows(CP, label, z1, fluids, fractions, floats, lines):
    """The rows of one mixture: an `err` row per state where the oracle cannot build or evaluate it."""
    for tau, delta in MIX_EOS_STATES:
        t = rho = math.nan
        try:
            state = CP.AbstractState("HEOS", fluids)
            if fractions is not None:
                state.set_mole_fractions(fractions)
            t, rho = state.T_reducing() / tau, delta * state.rhomolar_reducing()
            state.specify_phase(CP.iphase_gas)
            state.update(CP.DmolarT_INPUTS, rho, t)
            values = []
            for _, _, get in MIX_EOS_OUTPUTS:
                try:
                    values.append(get(state))
                except Exception:  # one output (w where the state is mechanically unstable): nan, the row stays
                    values.append(math.nan)
            status = "ok"
        except Exception as exception:  # a missing pair, an unknown component: a status, never a crash
            values, status = [math.nan] * len(MIX_EOS_OUTPUTS), f"err:{error_class(exception)}"
        floats.extend([z1, t, rho, *values])
        lines.append(",".join([label, cell(z1), cell(t), cell(rho), status, *map(cell, values)]) + "\n")


# Binaries whose critical points the oracle finds (map 04 #14: C1/C2's T_critical() throws, so the kind calls
# all_critical_points), at three compositions each.
MIX_CRIT_PAIRS = ["Methane&Ethane", "Methane&n-Propane", "Methane&n-Butane", "Ethane&n-Propane", "Nitrogen&Methane",
                  "CarbonDioxide&Methane", "R32&R125", "R32&R134a", "R125&R134a", "R134a&R1234yf"]
MIX_CRIT_Z1 = (0.25, 0.5, 0.75)
MIX_CRIT_COLUMNS = ["mixture", "z1", "index", "status", "T", "rhomolar", "p", "stable"]
MIX_CRIT_UNITS = ["-", "-", "-", "-", "K", "mol/m3", "Pa", "-"]
MIX_CRIT_TOL = ["label", "in", "in", "label", "flash", "flash", "flash", "label"]


def mix_crit(CP, lock, config, files, args):
    """The `mix_crit` kind (PLAN.md M13.7): all/mix_crit.csv, every critical point all_critical_points reports for
    MIX_CRIT_PAIRS at each MIX_CRIT_Z1, one row each with its `stable` flag; an `err` row where it throws."""
    lines, floats = [], []
    for label in MIX_CRIT_PAIRS:
        for z1 in MIX_CRIT_Z1:
            try:
                state = CP.AbstractState("HEOS", label)
                state.set_mole_fractions([z1, 1.0 - z1])
                points = sorted(state.all_critical_points(), key=lambda c: c.rhomolar)
                status = "ok"
            except Exception as exception:  # noqa: BLE001 - the oracle's refusal is a row
                points, status = [], f"err:{error_class(exception)}"
            if not points:
                floats.extend([z1, 0.0, math.nan, math.nan, math.nan])
                lines.append(",".join([label, cell(z1), cell(0.0), status, "nan", "nan", "nan", "none"]) + "\n")
            for index, c in enumerate(points):
                floats.extend([z1, float(index), c.T, c.rhomolar, c.p])
                row = [label, cell(z1), cell(float(index)), status, cell(c.T), cell(c.rhomolar), cell(c.p),
                       "stable" if c.stable else "unstable"]
                lines.append(",".join(row) + "\n")
    fluids = f"{len(MIX_CRIT_PAIRS)} pairs, fluids_sha256={lock['fluids_sha256']}, mixtures_sha256={lock['mixtures_sha256']}"
    grid = "z1 in {0.25,0.5,0.75}; every point all_critical_points returns, by density"
    text = header("mix_crit", lock, config, fluids, MIX_CRIT_COLUMNS, MIX_CRIT_TOL, floats, units=MIX_CRIT_UNITS,
                  grid=grid)
    return {"all/mix_crit.csv": text + "".join(lines)}


# The `mix_flash` kind (PLAN.md M13.8): CoolProp 8.0.0's PT flash of MIX_CRIT_PAIRS at each MIX_CRIT_Z1, at T = 0.7
# and 0.85 of the mixture's reducing temperature and three pressures from the oracle's own bubble and dew points
# there: 1.1 p_bub (a liquid), sqrt(p_bub p_dew) (a split) and 0.9 p_dew (a gas). Columns: Q (-1 for one phase),
# the bulk molar density and, for a split, the first component's liquid and vapour fractions.
MIX_FLASH_T = (0.7, 0.85)
MIX_FLASH_COLUMNS = ["mixture", "z1", "T", "p", "status", "Q", "rhomolar", "x1", "y1"]
MIX_FLASH_UNITS = ["-", "-", "K", "Pa", "-", "-", "mol/m3", "-", "-"]
MIX_FLASH_TOL = ["label", "in", "in", "in", "label", "flash", "flash", "flash", "flash"]


def mix_flash(CP, lock, config, files, args):
    """The `mix_flash` kind: all/mix_flash.csv, one row per PT state (an `err` row where the oracle throws; no row
    where it has no bubble or dew point at T, as above a mixture's cricondentherm)."""
    lines, floats = [], []
    for label in MIX_CRIT_PAIRS:
        for z1 in MIX_CRIT_Z1:
            for ratio in MIX_FLASH_T:
                state = CP.AbstractState("HEOS", label)
                state.set_mole_fractions([z1, 1.0 - z1])
                t = ratio * state.T_reducing()
                try:
                    state.update(CP.QT_INPUTS, 0.0, t)
                    p_bub = state.p()
                    state.update(CP.QT_INPUTS, 1.0, t)
                    p_dew = state.p()
                except Exception:  # noqa: BLE001 - no saturation at this T: no rows
                    continue
                for p in (1.1 * p_bub, math.sqrt(p_bub * p_dew), 0.9 * p_dew):
                    values = [math.nan] * 4
                    try:
                        flash = CP.AbstractState("HEOS", label)
                        flash.set_mole_fractions([z1, 1.0 - z1])
                        flash.update(CP.PT_INPUTS, p, t)
                        q = flash.Q()
                        two_phase = 0.0 < q < 1.0
                        x1 = flash.mole_fractions_liquid()[0] if two_phase else math.nan
                        y1 = flash.mole_fractions_vapor()[0] if two_phase else math.nan
                        values = [q if two_phase else -1.0, flash.rhomolar(), x1, y1]
                        status = "ok"
                    except Exception as exception:  # noqa: BLE001 - the oracle's refusal is a row
                        status = f"err:{error_class(exception)}"
                    floats.extend([z1, t, p, *values])
                    lines.append(",".join([label, cell(z1), cell(t), cell(p), status, *map(cell, values)]) + "\n")
    fluids = f"{len(MIX_CRIT_PAIRS)} pairs, fluids_sha256={lock['fluids_sha256']}, mixtures_sha256={lock['mixtures_sha256']}"
    grid = "z1 in {0.25,0.5,0.75}; T in {0.7,0.85} T_r; p in {1.1 p_bub, sqrt(p_bub p_dew), 0.9 p_dew}"
    text = header("mix_flash", lock, config, fluids, MIX_FLASH_COLUMNS, MIX_FLASH_TOL, floats, units=MIX_FLASH_UNITS,
                  grid=grid)
    return {"all/mix_flash.csv": text + "".join(lines)}


def mix_eos(CP, lock, config, files, args):
    """The `mix_eos` kind: all/mix_eos.csv over the library's pairs and predefined mixtures (any --tier)."""
    pairs, predefined = mixture_library(lock)
    lines, floats, seen = [], [], set()
    for pair in sorted(pairs, key=lambda p: tuple(sorted((p["CAS1"], p["CAS2"])))):
        assert_pair(CP, pair)
        mix_eos_rows(CP, f"{pair['Name1']}&{pair['Name2']}", 0.4, f"{pair['Name1']}&{pair['Name2']}", [0.4, 0.6],
                     floats, lines)
    for mixture in sorted(predefined, key=lambda m: m["name"]):
        if mixture["name"] in seen:
            continue
        seen.add(mixture["name"])
        mix_eos_rows(CP, f"{mixture['name']}.mix", math.nan, f"{mixture['name']}.mix", None, floats, lines)
    fluids = (f"{len(pairs)} pairs and {len(seen)} predefined mixtures, fluids_sha256={lock['fluids_sha256']}, "
              f"mixtures_sha256={lock['mixtures_sha256']}")
    grid = "pairs at z1 = 0.4; (tau, delta) in {(0.8,0.5),(1.25,2.0)} of each mixture's reducing state; gas imposed"
    text = header("mix_eos", lock, config, fluids, MIX_EOS_COLUMNS, MIX_EOS_TOL, floats, units=MIX_EOS_UNITS,
                  grid=grid)
    return {"all/mix_eos.csv": text + "".join(lines)}


# The `sat` kind (section 3.5). Since M5.2 it holds the `sa` rows: p, rho' and rho'' straight from
# `CP.SuperAncillary(json).eval_sat` (map 03 section 8); since M5.2a a QT row at Q = 0 at each `sa` temperature; since
# M6.8 a PQ row at Q = 1 at each of their pressures, and the core files: QT and PQ at both Q, no `sa` rows. A PQ row's
# T is CoolProp's answer, its p the input (the QT row's p at that temperature); CoolProp takes that T from its T(ln p)
# inverse without a polish (DIV-0018).
SAT_COLUMNS = ["input", "Q", "T", "status", "p", "rhoL", "rhoV", "hL", "hV", "sL", "sV", "path"]
SAT_UNITS = ["-", "-", "K", "-", "Pa", "mol/m3", "mol/m3", "J/mol", "J/mol", "J/mol/K", "J/mol/K", "-"]
SAT_TOL = ["label", "in", "in", "label", "sa_coeff", "sa_coeff", "sa_coeff", "prop", "prop", "prop", "prop", "label"]
SAT_ROWS = {"core": 25, "all": 8, "full": 200}  # temperatures per fluid
SAT_THETA_MIN = 1e-7  # Theta = 1 - T/Tc log-spaced from here to 1 - Tt/Tc
# A pseudo-pure fluid's grid stops further from its saturation maximum: R410A's ancillaries end at T_r = 344.494 K, short
# of its temperature_max_sat (344.4943434 K), where CoolProp's QT has no density seed (M6.9).
SAT_PSEUDO_THETA_MIN = 1e-3


def sat_sa_temperatures(t_min, t_max, rows):
    """`rows` temperatures with Theta = 1 - T/Tc log-spaced from SAT_THETA_MIN to 1 - Tt/Tc, Tt and Tc the
    superancillary's range, coldest first; each clamped into the range against rounding."""
    theta_max = 1.0 - t_min / t_max
    if rows == 1:
        return [t_min]
    thetas = [math.exp(math.log(theta_max) + (math.log(SAT_THETA_MIN) - math.log(theta_max)) * k / (rows - 1))
              for k in range(rows)]
    return [min(max(t_max * (1.0 - theta), t_min), t_max) for theta in thetas]


def sat_q_row(CP, name, given, q, x, path="superanc"):
    """One QT (`given` "T", x = T) or PQ (`given` "p", x = p, which is the row's p) row (map 03 section 8): T, p, rho',
    rho'', h', h'', s', s'' of a fresh `AbstractState`. On the superancillary's path CoolProp takes the densities from
    it and h, s from the EOS there (PLAN.md M5.2a, M6.8); on a pseudo-pure fluid's ancillary path each side's density
    is the EOS's at its ancillary pressure, and QT computes only the side Q names (Q = 0 or 1), so the other side's
    cells are nan (M6.9)."""
    both = path == "superanc" or given == "p"
    try:
        state = CP.AbstractState("HEOS", name)
        if given == "T":
            state.update(CP.QT_INPUTS, q, x)
        else:
            state.update(CP.PQ_INPUTS, x, q)
        sides = [(state.saturated_liquid_keyed_output, both or q == 0.0),
                 (state.saturated_vapor_keyed_output, both or q == 1.0)]
        cells = [[read(key) if on else math.nan for read, on in sides] for key in (CP.iDmolar, CP.iHmolar, CP.iSmolar)]
        values = [state.T(), state.p() if given == "T" else x, *[v for pair in cells for v in pair]]
        status = "ok"
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        values = [x if given == "T" else math.nan, x if given == "p" else math.nan, *[math.nan] * 6]
        status = f"err:{error_class(exception)}"
    row = [q, *values]
    return row, ",".join([given, cell(q), cell(values[0]), status, *map(cell, values[1:]), path]) + "\n"


def sat_rows(CP, name, superancillary, tier, rows):
    """The `sat` rows of one fluid at `rows` temperatures: (floats, lines). The all-fluid tier and the full set: the
    `sa` rows, then a QT row at Q = 0 at each temperature (M5.2a), then a PQ row at Q = 1 at each of their pressures
    (M6.8). The core subset: at each temperature QT at Q = 0 and 1, then PQ at both Q at the QT rows' pressure."""
    pieces = superancillary["jexpansions_p"]
    temperatures = sat_sa_temperatures(pieces[0]["xmin"], pieces[-1]["xmax"], rows)
    lines, floats = [], []

    def add(row, line):
        floats.extend(row)
        lines.append(line)

    if tier == "core":
        for t in temperatures:
            qt = [sat_q_row(CP, name, "T", q, t) for q in (0.0, 1.0)]
            for row, line in qt:
                add(row, line)
            for q in (0.0, 1.0):
                add(*sat_q_row(CP, name, "p", q, qt[0][0][2]))
        return floats, lines
    sa = CP.SuperAncillary(json.dumps(superancillary))
    for t in temperatures:
        try:
            values, status = [sa.eval_sat(t, "P", 0), sa.eval_sat(t, "D", 0), sa.eval_sat(t, "D", 1)], "ok"
        except Exception as exception:  # every oracle failure becomes a status, never a crash
            values, status = [math.nan] * 3, f"err:{error_class(exception)}"
        row = [math.nan, t, *values, *[math.nan] * 4]
        add(row, ",".join(["sa", "nan", cell(t), status, *map(cell, row[2:]), "superanc"]) + "\n")
    qt = [sat_q_row(CP, name, "T", 0.0, t) for t in temperatures]
    for row, line in qt:
        add(row, line)
    for row, _ in qt:
        add(*sat_q_row(CP, name, "p", 1.0, row[2]))
    return floats, lines


def sat_pseudo_rows(CP, name, states, rows):
    """A pseudo-pure fluid's `sat` rows (M6.9; D4, map 04 U4): at `rows` temperatures, Theta = 1 - T/T_max log-spaced
    from SAT_PSEUDO_THETA_MIN to the low end, T_max its temperature_max_sat and the low end the higher of its
    sat_min_liquid and sat_min_vapor (CoolProp's QT range), QT at Q = 0 and 1, then PQ at Q = 0, 0.5 and 1 at the
    bubble pressure of that temperature (the QT row's at Q = 0, none where it failed); path `ancillary`: (floats,
    lines)."""
    t_min = max(states["sat_min_liquid"]["T"], states["sat_min_vapor"]["T"])
    t_max = states["temperature_max_sat"]["T"]
    theta_max = 1.0 - t_min / t_max
    thetas = [math.exp(math.log(theta_max) + (math.log(SAT_PSEUDO_THETA_MIN) - math.log(theta_max)) * k / (rows - 1))
              for k in range(rows)] if rows > 1 else [theta_max]
    lines, floats = [], []
    for t in [min(max(t_max * (1.0 - theta), t_min), t_max) for theta in thetas]:
        qt = [sat_q_row(CP, name, "T", q, t, "ancillary") for q in (0.0, 1.0)]
        bubble = qt[0][0][2]  # nan where the oracle's QT failed: no p to give PQ
        pq = [] if math.isnan(bubble) else [sat_q_row(CP, name, "p", q, bubble, "ancillary") for q in (0.0, 0.5, 1.0)]
        for row, line in qt + pq:
            floats.extend(row)
            lines.append(line)
    return floats, lines


def sat(CP, lock, config, files, args):
    """The `sat` kind (section 3.5) for the fluids with a superancillary and the pseudo-pure ones: the core subset's
    files (sat/<Fluid>.csv, 100 rows each, 125 for a pseudo-pure fluid), all/sat.csv (8 temperatures per fluid) and the
    full set's files (200 temperatures)."""
    rows = args.rows or SAT_ROWS[args.tier]
    results, shas, pseudo = [], {}, 0
    for name in tier_fluids(args, files):
        eos = json.loads(files[name].read_text(encoding="utf-8"))["EOS"][0]
        superancillary = eos.get("SUPERANCILLARY")
        if superancillary is None and not eos["pseudo_pure"]:
            continue
        shas[name] = assert_fluid(CP, files, name)
        if superancillary is None:
            pseudo += 1
            results.append((name, *sat_pseudo_rows(CP, name, eos["STATES"], rows)))
        else:
            results.append((name, *sat_rows(CP, name, superancillary, args.tier, rows)))
    if args.tier == "core":
        grid = (f"input=T,p Q=0,1 Theta=1-T/Tc logspace[{SAT_THETA_MIN!r},1-Tt/Tc] n={rows} "
                "Tt,Tc=superancillary range; p=QT(Q=0) p at each T")
    else:
        grid = (f"input=sa,T(Q=0),p(Q=1) Theta=1-T/Tc logspace[{SAT_THETA_MIN!r},1-Tt/Tc] n={rows} "
                "Tt,Tc=superancillary range; p=QT p at each T")
    pseudo_grid = (f"pseudo-pure: input=T Q=0,1, p Q=0,0.5,1 Theta=1-T/Tmax logspace[{SAT_PSEUDO_THETA_MIN!r},"
                   f"1-Tmin/Tmax] n={rows} Tmin,Tmax=QT range; p=QT(Q=0) p at each T")
    if args.tier == "all":
        grid = f"{grid}; {pseudo_grid}"
        floats = [value for _, values, _ in results for value in values]
        fluids = (f"{len(results) - pseudo} with a superancillary and {pseudo} pseudo-pure, "
                  f"fluids_sha256={lock['fluids_sha256']}")
        columns, tol, units = ["fluid", *SAT_COLUMNS], ["label", *SAT_TOL], ["-", *SAT_UNITS]
        text = header("sat", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        return {"all/sat.csv": text + "".join(f"{name},{line}" for name, _, lines in results for line in lines)}
    out = {}
    for name, floats, lines in results:
        own = pseudo_grid if lines and lines[0].endswith(",ancillary\n") else grid
        text = header("sat", lock, config, [(name, shas[name])], SAT_COLUMNS, SAT_TOL, floats, units=SAT_UNITS,
                      grid=own)
        out[f"sat/{name}.csv"] = text + "".join(lines)
    return out


# The `flash` kind (section 3.5): pairs read off truth states, never density bands (map 12 section 6.4). Since M5.3
# the DT rows: a truth state on the (log p, T) grid or the (T, Q) grid, its density and T, and CoolProp's DT flash of
# them with no phase imposed. Since M7.1 the PT rows: each (p, T) truth itself, CoolProp's PT flash of its own inputs,
# ahead of its DT row. x1 and x2 are the pair's inputs in SI molar units (DT: mol/m3, K; PT: Pa, K). Q is nan for a
# single phase (CoolProp's -1 sentinel, ROT-013, and its Q = 1 on a pseudo-pure gas); `phase` is CoolProp's name.
FLASH_COLUMNS = ["pair", "x1", "x2", "truth", "status", "T", "rho", "p", "h", "s", "u", "Q", "phase"]
FLASH_UNITS = ["-", "pair", "K", "-", "-", "K", "mol/m3", "Pa", "J/mol", "J/mol/K", "J/mol", "-", "-"]
FLASH_TOL = ["label", "in", "in", "label", "label", *["flash"] * 7, "label"]
FLASH_GRID = {"core": (6, 4), "all": (0, 4), "full": (40, 20)}  # PT points per axis, QT points per axis
FLASH_PHASES = {"phase_liquid": "liquid", "phase_gas": "gas", "phase_twophase": "twophase",
                "phase_supercritical": "supercritical", "phase_supercritical_gas": "supercritical_gas",
                "phase_supercritical_liquid": "supercritical_liquid", "phase_critical_point": "critical_point"}


def flash_truths(CP, name, n_pt, n_qt):
    """The truth states of one fluid: n_pt x n_pt (p, T) at cell centres of T in [T_low, Tmax] and log p in
    [max(ptriple, 1 Pa), pmax], then n_qt x n_qt (T, Q) at cell centres of T in [T_low, Tc] and Q in (0, 1); T_low =
    max(Tmin, Ttriple) (map 09 R8). Each is (label, CoolProp input pair, value 1, value 2). CoolProp has no QT at
    0 < Q < 1 for a pseudo-pure fluid, so its (T, Q) cells are `dome` truths instead (M6.9): (rho, T) given, rho at
    1/rho = Q/rho'' + (1 - Q)/rho' on its QT densities at Q = 0 and 1 (pair None)."""
    state = CP.AbstractState("HEOS", name)
    t_low, t_max, tc = max(state.Tmin(), state.Ttriple()), state.Tmax(), state.T_critical()
    p_low, p_max = max(state.p_triple(), 1.0), state.pmax()
    centre = lambda k, n: (k + 0.5) / n  # noqa: E731
    pseudo_pure = state.fluid_param_string("pure") == "false"
    truths = []
    for i in range(n_pt):
        t = t_low + (t_max - t_low) * centre(i, n_pt)
        for j in range(n_pt):
            p = math.exp(math.log(p_low) + (math.log(p_max) - math.log(p_low)) * centre(j, n_pt))
            truths.append(("PT", CP.PT_INPUTS, p, t))
    for i in range(n_qt):
        t = t_low + (tc - t_low) * centre(i, n_qt)
        if not pseudo_pure:
            truths.extend(("QT", CP.QT_INPUTS, centre(j, n_qt), t) for j in range(n_qt))
            continue
        try:
            sides = []
            for q in (0.0, 1.0):
                side = CP.AbstractState("HEOS", name)
                side.update(CP.QT_INPUTS, q, t)
                sides.append(side.rhomolar())
        except Exception:  # noqa: BLE001 - no definition at this T; no truths to make
            continue
        for j in range(n_qt):
            q = centre(j, n_qt)
            truths.append(("dome", None, 1.0 / (q / sides[1] + (1.0 - q) / sides[0]), t))
    return truths


def flash_phase(CP, state):
    """CoolProp's name of the phase of `state`."""
    return next((v for k, v in FLASH_PHASES.items() if CP.get_phase_index(k) == state.phase()), "other")


def flash_rows(job):
    """One fluid's rows, in a pool child: for a (p, T) truth its PT row (the truth state as CoolProp's PT flash left it),
    then for every truth its (rho, T) and a fresh `AbstractState` DT flash. A truth the oracle cannot make (a PT point
    below the melting line, say) is skipped; a failed DT flash is a status."""
    import CoolProp.CoolProp as CP  # the parent's module, inherited through fork

    name, n_pt, n_qt = job
    lines, floats = [], []
    for truth, pair, v1, v2 in flash_truths(CP, name, n_pt, n_qt):
        try:
            if pair is None:
                rho, t = v1, v2
            else:
                state = CP.AbstractState("HEOS", name)
                state.update(pair, v1, v2)
                rho, t = state.rhomolar(), state.T()
        except Exception:  # noqa: BLE001 - no truth state there; nothing to read off
            continue
        if truth == "PT":
            values = [state.T(), rho, state.p(), state.hmolar(), state.smolar(), state.umolar(), math.nan]
            floats.extend([v1, v2, *values])
            lines.append(",".join(["PT", cell(v1), cell(v2), truth, "ok", *map(cell, values), flash_phase(CP, state)])
                         + "\n")
        try:
            state = CP.AbstractState("HEOS", name)
            state.update(CP.DmolarT_INPUTS, rho, t)
            phase = flash_phase(CP, state)
            # Q of a two-phase state only: CoolProp leaves Q = 1 on a gas its pseudo-pure VLE path labels (M6.9), a
            # sentinel like its -1 (ROT-013).
            q = state.Q() if phase == "twophase" else math.nan
            values = [state.T(), state.rhomolar(), state.p(), state.hmolar(), state.smolar(), state.umolar(), q]
            status = "ok"
        except Exception as exception:  # every oracle failure becomes a status, never a crash
            values, phase, status = [math.nan] * 7, "none", f"err:{error_class(exception)}"
        floats.extend([rho, t, *values])
        lines.append(",".join(["DT", cell(rho), cell(t), truth, status, *map(cell, values), phase]) + "\n")
    return lines, floats


def flash(CP, lock, config, files, args):
    """The `flash` kind (section 3.5), DT rows (M5.3) and PT rows (M7.1): flash/<Fluid>.csv for each core fluid (6 x 6
    PT + 4 x 4 QT truths) and the full set (40 x 40 + 20 x 20); all/flash.csv holds the six pseudo-pure fluids' 4 x 4
    `dome` truths (M6.9)."""
    n_pt, n_qt = FLASH_GRID[args.tier]
    names = tier_fluids(args, files)
    if args.tier == "all":
        names = [name for name in names if json.loads(files[name].read_text(encoding="utf-8"))["EOS"][0]["pseudo_pure"]]
    shas = {name: assert_fluid(CP, files, name) for name in names}
    results = pool_map(flash_rows, [(name, n_pt, n_qt) for name in names], args.jobs)
    grid = (f"truth PT {n_pt}x{n_pt} T~[Tlow,Tmax] log p~[max(ptriple,1 Pa),pmax]; truth QT {n_qt}x{n_qt} "
            "T~[Tlow,Tc] Q~(0,1), dome for a pseudo-pure fluid (rho on its QT densities); cell centres; pair PT of each "
            "PT truth, pair DT read off each truth, no phase imposed")
    if args.tier == "all":
        floats = [value for _, values in results for value in values]
        fluids = f"{len(names)} pseudo-pure, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *FLASH_COLUMNS], ["label", *FLASH_TOL], ["-", *FLASH_UNITS]
        text = header("flash", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        body = "".join(f"{name},{line}" for name, (lines, _) in zip(names, results) for line in lines)
        return {"all/flash.csv": text + body}
    out = {}
    for name, (lines, floats) in zip(names, results):
        text = header("flash", lock, config, [(name, shas[name])], FLASH_COLUMNS, FLASH_TOL, floats, units=FLASH_UNITS,
                      grid=grid)
        out[f"flash/{name}.csv"] = text + "".join(lines)
    return out



# The `sigma` kind (section 3.5): CoolProp's surface tension of its saturated liquid at T, sampled uniformly over the
# fit's range [Tmin, Tc], Tc the fit's own (ANCILLARIES.surface_tension.Tc, Mulero's) and Tmin CoolProp's. CoolProp needs
# a two-phase state, so a T above the EOS's critical point is an err row (map 05 R13).
SIGMA_COLUMNS = ["T", "status", "sigma"]
SIGMA_UNITS = ["K", "-", "N/m"]
SIGMA_TOL = ["in", "label", "prop"]
SIGMA_ROWS = {"core": 50, "all": 4, "full": 500}  # temperatures per fluid


def sigma_rows(CP, name, tc, rows, seed):
    """One fluid's rows: (floats, lines), its temperatures from its own SplitMix64 stream."""
    state = CP.AbstractState("HEOS", name)
    t_min, rng = state.Tmin(), splitmix64(seed)
    floats, lines = [], []
    for _ in range(rows):
        t = uniform(rng, t_min, tc)
        try:
            state.update(CP.QT_INPUTS, 0, t)
            value, status = state.surface_tension(), "ok"
        except Exception as exception:  # every oracle failure becomes a status, never a crash
            value, status = math.nan, f"err:{error_class(exception)}"
        floats += [t, value]
        lines.append(f"{cell(t)},{status},{cell(value)}\n")
    return floats, lines


def sigma(CP, lock, config, files, args):
    """The `sigma` kind for the fluids with a surface-tension fit: sigma/<Fluid>.csv in the core tier (50 rows),
    all/sigma.csv (4 per fluid) and the full set's files (500)."""
    rows = args.rows or SIGMA_ROWS[args.tier]
    results, shas = [], {}
    for name in tier_fluids(args, files):
        fit = json.loads(files[name].read_text(encoding="utf-8"))["ANCILLARIES"].get("surface_tension")
        if fit is None:
            continue
        shas[name] = assert_fluid(CP, files, name)
        results.append((name, *sigma_rows(CP, name, fit["Tc"], rows, args.seed)))
    grid = f"T~U[Tmin,Tc] n={rows} Tc=the fit's own (ANCILLARIES.surface_tension.Tc); saturated liquid (QT, Q=0) at T"
    if args.tier == "all":
        floats = [value for _, values, _ in results for value in values]
        fluids = f"{len(results)} with a surface-tension fit, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *SIGMA_COLUMNS], ["label", *SIGMA_TOL], ["-", *SIGMA_UNITS]
        text = header("sigma", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        return {"all/sigma.csv": text + "".join(f"{name},{line}" for name, _, lines in results for line in lines)}
    out = {}
    for name, floats, lines in results:
        text = header("sigma", lock, config, [(name, shas[name])], SIGMA_COLUMNS, SIGMA_TOL, floats, units=SIGMA_UNITS,
                      grid=grid)
        out[f"sigma/{name}.csv"] = text + "".join(lines)
    return out


# The `melt` kind (section 3.5): CoolProp's melting line per segment of ANCILLARIES.melting_line, half the rows p(T) at
# T uniform over the segment's range and half T(p) at p log-uniform over its pressures, each segment's p range from its
# own formula at its two bounds (CoolProp's p(T) answers from the first segment holding T, which on Water's ice III and
# V ranges is ice Ih's). Every row lies inside its segment: CoolProp's Simon T(p) below T_min (ROT-049) is not sampled.
MELT_COLUMNS = ["input", "T", "p", "status"]
MELT_UNITS = ["-", "K", "Pa", "-"]
MELT_TOL = ["label", "prop", "prop", "label"]
MELT_ROWS = {"core": 50, "all": 4, "full": 500}  # rows per segment


def melt_p(kind, part, t):
    """p at `t` on one part, as `Ancillaries.h:200-244` computes it."""
    if kind == "Simon":
        return part["p_0"] + part["a"] * ((t / part["T_0"]) ** part["c"] - 1)
    if kind == "polynomial_in_Tr":
        return part["p_0"] * (1 + sum(a * ((t / part["T_0"]) ** n - 1) for a, n in zip(part["a"], part["t"])))
    return part["p_0"] * (1 + sum(a * (t / part["T_0"] - 1) ** n for a, n in zip(part["a"], part["t"])))


def melt_rows(CP, name, line, rows, seed):
    """One fluid's rows: (floats, lines), each segment's values from its own SplitMix64 stream."""
    state = CP.AbstractState("HEOS", name)
    floats, lines = [], []
    for k, part in enumerate(line["parts"]):
        rng = splitmix64(seed + k)
        t_lo, t_hi = sorted((part["T_min"], part["T_max"]))
        p_lo, p_hi = sorted((melt_p(line["type"], part, t_lo), melt_p(line["type"], part, t_hi)))
        for i in range(rows):
            given = "T" if i < rows // 2 else "p"
            if given == "T":
                t, p = uniform(rng, t_lo, t_hi), math.nan
            else:
                t, p = math.nan, math.exp(uniform(rng, math.log(p_lo), math.log(p_hi)))
            try:
                if given == "T":
                    p = state.melting_line(CP.iP, CP.iT, t)
                else:
                    t = state.melting_line(CP.iT, CP.iP, p)
                status = "ok"
            except Exception as exception:  # every oracle failure becomes a status, never a crash
                status = f"err:{error_class(exception)}"
            floats += [t, p]
            lines.append(f"{given},{cell(t)},{cell(p)},{status}\n")
    return floats, lines


def melt(CP, lock, config, files, args):
    """The `melt` kind for the fluids with a melting line: melt/<Fluid>.csv in the core tier (50 rows per segment),
    all/melt.csv (4) and the full set's files (500)."""
    rows = args.rows or MELT_ROWS[args.tier]
    results, shas = [], {}
    for name in tier_fluids(args, files):
        line = json.loads(files[name].read_text(encoding="utf-8"))["ANCILLARIES"].get("melting_line")
        if line is None:
            continue
        shas[name] = assert_fluid(CP, files, name)
        results.append((name, *melt_rows(CP, name, line, rows, args.seed)))
    grid = (f"per segment of ANCILLARIES.melting_line: n={rows // 2} p(T), T~U[segment T range]; n={rows - rows // 2} "
            "T(p), p~logU[segment p range, from its formula at its bounds]")
    if args.tier == "all":
        floats = [value for _, values, _ in results for value in values]
        fluids = f"{len(results)} with a melting line, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *MELT_COLUMNS], ["label", *MELT_TOL], ["-", *MELT_UNITS]
        text = header("melt", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        return {"all/melt.csv": text + "".join(f"{name},{line}" for name, _, lines in results for line in lines)}
    out = {}
    for name, floats, lines in results:
        text = header("melt", lock, config, [(name, shas[name])], MELT_COLUMNS, MELT_TOL, floats, units=MELT_UNITS,
                      grid=grid)
        out[f"melt/{name}.csv"] = text + "".join(lines)
    return out


# The `transport` kind (section 3.5): CoolProp's viscosity and conductivity, totals and contributions, at stable
# single-phase states (map 10 section 8.5): T uniform in [T_low, Tmax] and p log-uniform in [max(ptriple, 1 Pa), pmax]
# from a SplitMix64 stream, rho from CoolProp's PT flash there, then transport on a fresh DT state at (T, rho), so a
# model reading p reads the EOS's p(T, rho) (map 05 R14). The contributions are CoolProp's `*_contributions`, which
# report a whole model's total as `critical` (map 05 R8); the model columns name each property's kind.
TRANSPORT_STAGES = ["dilute", "initial_density", "residual", "critical"]
TRANSPORT_COLUMNS = ["T", "rho", "phase",
                     "eta_status", "eta", *(f"eta_{s}" for s in TRANSPORT_STAGES),
                     "lambda_status", "lambda", *(f"lambda_{s}" for s in TRANSPORT_STAGES), "eta_model", "lambda_model"]
TRANSPORT_UNITS = ["K", "mol/m3", "-", "-", *["Pa s"] * 5, "-", *["W/(m K)"] * 5, "-", "-"]
TRANSPORT_TOL = ["in", "in", "label", "label", *["transport_direct"] * 5, "label", *["transport_direct"] * 5, "label",
                 "label"]
TRANSPORT_ROWS = {"core": 200, "all": 8, "full": 5000}  # states per fluid


def transport_model(block):
    """A model's kind from its JSON block (a list's first entry): staged, ecs, rhosr, chung, hardcoded, or none."""
    if block is None:
        return "none"
    block = block[0] if isinstance(block, list) else block
    kind = {"ECS": "ecs", "rhosr-CS": "rhosr", "Chung": "chung"}.get(block.get("type"))
    return kind or ("hardcoded" if "hardcoded" in block else "staged")


def transport_values(state, prop):
    """A property's total and contributions at `state`: (status, five values)."""
    try:
        total = getattr(state, prop)()
        parts = getattr(state, f"{prop}_contributions")()
        return "ok", [total, *(parts[s] for s in TRANSPORT_STAGES)]
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        return f"err:{error_class(exception)}", [math.nan] * 5


def transport_rows(job):
    """One fluid's rows, in a pool child: (floats, lines). A (T, p) the PT flash cannot make gives no row."""
    import CoolProp.CoolProp as CP  # the parent's module, inherited through fork

    name, rows, seed, models = job
    flash_state = CP.AbstractState("HEOS", name)
    t_low, t_max = max(flash_state.Tmin(), flash_state.Ttriple()), flash_state.Tmax()
    p_low, p_max = max(flash_state.p_triple(), 1.0), flash_state.pmax()
    rng = splitmix64(seed)
    floats, lines = [], []
    for _ in range(rows):
        t, p = uniform(rng, t_low, t_max), log_uniform(rng, p_low, p_max)
        try:
            flash_state.update(CP.PT_INPUTS, p, t)
            rho = flash_state.rhomolar()
            state = CP.AbstractState("HEOS", name)
            state.update(CP.DmolarT_INPUTS, rho, t)
            phase = flash_phase(CP, state)
        except Exception:  # noqa: BLE001 - no state at this (T, p): no row
            continue
        transport_line(state, t, rho, phase, models, floats, lines)
    return floats, lines


def transport_line(state, t, rho, phase, models, floats, lines):
    """One row's floats and line at a DT `state` (T, rho): totals, contributions, statuses and model kinds."""
    eta_status, eta = transport_values(state, "viscosity")
    lambda_status, conductivity = transport_values(state, "conductivity")
    floats += [t, rho, *eta, *conductivity]
    cells = [cell(t), cell(rho), phase, eta_status, *map(cell, eta), lambda_status, *map(cell, conductivity), *models]
    lines.append(",".join(cells) + "\n")


# Fixed DT states for the branches no random state reaches (`--set probes`, M8.6), (T, rho in mol/m3) per fluid.
# Friend's methane conductivity reads the saturated vapour's density in the vapour below the critical point, and
# picks its chi_T* by Eq. 26 (rho* = 1 - rho/10139 = 0), Eq. 23 (|rho*| < 0.03) or Eq. 19a within |T*| < 0.03 of
# 190.55 K; a liquid and a supercritical state close the set. Helium's conductivity has a critical term for
# 3.5 < T < 12 K (M8.9): seven states blend the scaled equation's kappa_T (W <= 1) outside and between the two lobes
# where the blend is negative (DIV-0035), four use kappa_T alone (W > 1), and two sit on 3.5 and 12 K, where the term
# is off. Every state is single phase.
TRANSPORT_PROBES = {
    "Methane": [
        (120.0, 20.0), (150.0, 400.0), (170.0, 1500.0), (185.0, 4986.0),
        (191.0, 10139.0), (193.0, 10139.0),
        (191.0, 10000.0), (192.0, 10350.0), (195.0, 9900.0),
        (191.0, 9000.0), (194.0, 11500.0),
        (100.0, 28000.0), (300.0, 5000.0),
    ],
    "Helium": [
        (5.3, 15000.0), (6.0, 19000.0), (5.4, 21000.0), (5.8, 14500.0), (5.8, 20500.0), (6.1, 17278.0), (5.5, 17300.0),
        (5.6, 12500.0), (8.0, 10000.0), (4.5, 1500.0), (4.0, 33000.0),
        (3.5, 1000.0), (12.0, 17000.0),
    ],
}


def transport_probes(CP, lock, config, files):
    """`transport/probes.csv`: the kind's columns at TRANSPORT_PROBES's states, each a fresh DT state."""
    floats, lines, shas = [], [], []
    for name, states in TRANSPORT_PROBES.items():
        shas.append((name, assert_fluid(CP, files, name)))
        block = json.loads(files[name].read_text(encoding="utf-8")).get("TRANSPORT") or {}
        models = [transport_model(block.get("viscosity")), transport_model(block.get("conductivity"))]
        for t, rho in states:
            state = CP.AbstractState("HEOS", name)
            state.update(CP.DmolarT_INPUTS, rho, t)
            row = []
            transport_line(state, t, rho, flash_phase(CP, state), models, floats, row)
            lines += [f"{name},{line}" for line in row]
    columns, tol, units = ["fluid", *TRANSPORT_COLUMNS], ["label", *TRANSPORT_TOL], ["-", *TRANSPORT_UNITS]
    grid = "TRANSPORT_PROBES: fixed DT states for the branches no random state reaches"
    text = header("transport", lock, config, shas, columns, tol, floats, units=units, grid=grid)
    return {"transport/probes.csv": text + "".join(lines)}


def transport(CP, lock, config, files, args):
    """The `transport` kind: transport/<Fluid>.csv for the core subset (200 states each), all/transport.csv (8 per
    fluid) and the full set's files (5000); with `--set probes`, transport/probes.csv."""
    if args.set == "probes":
        return transport_probes(CP, lock, config, files)
    rows = args.rows or TRANSPORT_ROWS[args.tier]
    names = tier_fluids(args, files)
    jobs, shas = [], {}
    for name in names:
        shas[name] = assert_fluid(CP, files, name)
        block = json.loads(files[name].read_text(encoding="utf-8")).get("TRANSPORT") or {}
        models = [transport_model(block.get("viscosity")), transport_model(block.get("conductivity"))]
        jobs.append((name, rows, args.seed, models))
    results = pool_map(transport_rows, jobs, args.jobs)
    grid = (f"T~U[Tlow,Tmax] log p~U[max(ptriple,1 Pa),pmax] n={rows}, Tlow=max(Tmin,Ttriple); rho from the PT flash; "
            "transport at DT(rho, T); stable single phase only")
    if args.tier == "all":
        floats = [value for values, _ in results for value in values]
        fluids = f"{len(names)} fluids, fluids_sha256={lock['fluids_sha256']}"
        columns, tol, units = ["fluid", *TRANSPORT_COLUMNS], ["label", *TRANSPORT_TOL], ["-", *TRANSPORT_UNITS]
        text = header("transport", lock, config, fluids, columns, tol, floats, units=units, grid=grid)
        body = "".join(f"{name},{line}" for name, (_, lines) in zip(names, results) for line in lines)
        return {"all/transport.csv": text + body}
    out = {}
    for name, (floats, lines) in zip(names, results):
        text = header("transport", lock, config, [(name, shas[name])], TRANSPORT_COLUMNS, TRANSPORT_TOL, floats,
                      units=TRANSPORT_UNITS, grid=grid)
        out[f"transport/{name}.csv"] = text + "".join(lines)
    return out


# The `refstate` kind (section 3.5): h and s in CoolProp's named reference states (map 01 section 3), each set with
# `set_reference_state` and reset to DEF after its rows: it edits the process-wide fluid library, and an AbstractState
# made before keeps its old offsets (map 01 R14). Per reference state: its anchor (IIR: saturated liquid at 273.15 K,
# defined as h = 200 kJ/kg and s = 1 kJ/(kg K); ASHRAE: saturated liquid at 233.15 K, h = s = 0; NBP: saturated liquid at
# 101325 Pa, h = s = 0), then REFSTATE_ROWS saturated states, T uniform over [Tlow, Tlow + 0.9 (Tc - Tlow)] and Q
# alternating 0 and 1, each on a fresh AbstractState. A reference state CoolProp refuses gives one anchor row with its
# error status. x1 and x2 are the pair's inputs in SI units (QT: Q, K; PQ: Pa, Q). The
# fluids: the core subset and the three of CoolProp's own reference-state test (`CoolProp-Tests.cpp:2263-2436`); the
# kind has no all-fluid tier, and its full set is the core one.
REFSTATE_COLUMNS = ["refstate", "role", "pair", "x1", "x2", "status", "h_mass", "s_mass"]
REFSTATE_UNITS = ["-", "-", "-", "pair", "pair", "-", "J/kg", "J/kg/K"]
REFSTATE_TOL = ["label", "label", "label", "in", "in", "label", "prop", "prop"]
REFSTATE_ANCHORS = [("IIR", "QT", 0.0, 273.15), ("ASHRAE", "QT", 0.0, 233.15), ("NBP", "PQ", 101325.0, 0.0)]
REFSTATE_ROWS = 20  # saturated states per reference state
REFSTATE_TEST_FLUIDS = ["R124", "R134a", "n-Propane"]


def refstate_value(CP, name, pair, x1, x2):
    """(h_mass, s_mass, status) on a fresh state at the pair's inputs, in the reference state currently set."""
    try:
        state = CP.AbstractState("HEOS", name)
        state.update(getattr(CP, f"{pair}_INPUTS"), x1, x2)
        return state.hmass(), state.smass(), "ok"
    except Exception as exception:  # every oracle failure becomes a status, never a crash
        return math.nan, math.nan, f"err:{error_class(exception)}"


def refstate_rows(CP, name, rows, seed):
    """One fluid's rows: (floats, lines), each reference state's temperatures from its own SplitMix64 stream."""
    probe = CP.AbstractState("HEOS", name)
    t_low = max(probe.Tmin(), probe.Ttriple())
    t_high = t_low + 0.9 * (probe.T_critical() - t_low)
    floats, lines = [], []
    for k, (ref, pair, x1, x2) in enumerate(REFSTATE_ANCHORS):
        rng = splitmix64(seed + k)
        try:
            CP.set_reference_state(name, ref)
        except Exception as exception:  # a refused reference state is a row, never a crash
            floats += [x1, x2, math.nan, math.nan]
            lines.append(f"{ref},anchor,{pair},{cell(x1)},{cell(x2)},err:{error_class(exception)},nan,nan\n")
            continue
        try:
            cases = [("anchor", pair, x1, x2)]
            cases += [("state", "QT", float(i % 2), uniform(rng, t_low, t_high)) for i in range(rows)]
            for role, case_pair, y1, y2 in cases:
                h, s, status = refstate_value(CP, name, case_pair, y1, y2)
                floats += [y1, y2, h, s]
                lines.append(f"{ref},{role},{case_pair},{cell(y1)},{cell(y2)},{status},{cell(h)},{cell(s)}\n")
        finally:
            CP.set_reference_state(name, "DEF")
    return floats, lines


def refstate(CP, lock, config, files, args):
    """The `refstate` kind: refstate/<Fluid>.csv for the core subset and REFSTATE_TEST_FLUIDS (one anchor and
    REFSTATE_ROWS saturated states per reference state), the same files in the full set."""
    if args.tier == "all":
        fail("--kind refstate has no all-fluid tier (section 3.5)")
    rows = args.rows or REFSTATE_ROWS
    names = sorted(CORE_FLUIDS + REFSTATE_TEST_FLUIDS)
    if args.fluids:
        unknown = [name for name in args.fluids.split(",") if name not in names]
        if unknown:
            fail(f"--fluids: not refstate fluids: {', '.join(unknown)}")
        names = [name for name in names if name in args.fluids.split(",")]
    grid = (f"per reference state: its anchor, then n={rows} QT states, T~U[Tlow,Tlow+0.9(Tc-Tlow)], "
            "Tlow=max(Tmin,Ttriple), Q alternating 0,1")
    out = {}
    for name in names:
        sha = assert_fluid(CP, files, name)
        floats, lines = refstate_rows(CP, name, rows, args.seed)
        text = header("refstate", lock, config, [(name, sha)], REFSTATE_COLUMNS, REFSTATE_TOL, floats,
                      units=REFSTATE_UNITS, grid=grid)
        out[f"refstate/{name}.csv"] = text + "".join(lines)
    return out


# The `props` kind (section 3.5): CoolProp's 85 outputs (map 01 section 4a, in `parameter_info_list` order) through
# `PropsSI` at direct states inside phasekit's domain, long format. Per fluid, PROPS_STATES states from one SplitMix64
# stream: the first half DT, T ~ U[Tlow, Tmax] and rho ~ logU[1e-6 rhoL(Tlow), rhoL(Tlow)] with no phase imposed (so
# some lie in the dome), drawn again where CoolProp's p there exceeds pmax (the kind checks the outputs, not the domain
# policy, D6); the second half QT, T ~ U[Tlow, Tlow + 0.9 (Tc - Tlow)] with Q cycling 0, 0.5, 1; Tlow the higher of
# Tmin and the EOS's own Ttriple (CoolProp's Ttriple is its Tmin, map 09 R8); every output at each; then the 28 trivial
# outputs through `Props1SI` (pair `Props1SI`, x1 = x2 = nan), whose failures are an inf return with the errstring set
# (`call`). x1 and x2 are the pair's inputs in SI molar units (DT: mol/m3, K; QT: Q, K). A refused output is
# `err:<class>` with value nan.
PROPS_OUTPUTS = [
    "T", "P", "Dmolar", "Hmolar", "Smolar", "Umolar", "Gmolar", "Helmholtzmolar", "Dmass", "Hmass", "Smass", "Umass",
    "Gmass", "Helmholtzmass", "Q", "Qmass", "Delta", "Tau", "Cpmolar", "Cpmass", "Cvmolar", "Cvmass", "Cp0molar",
    "Cp0mass", "Hmolar_residual", "Smolar_residual", "Gmolar_residual", "Hmolar_idealgas", "Smolar_idealgas",
    "Umolar_idealgas", "Hmass_idealgas", "Smass_idealgas", "Umass_idealgas", "GWP20", "GWP100", "GWP500", "FH", "HH",
    "PH", "ODP", "Bvirial", "Cvirial", "dBvirial_dT", "dCvirial_dT", "gas_constant", "molar_mass", "acentric",
    "dipole_moment", "rhomass_reducing", "rhomolar_reducing", "rhomolar_critical", "rhomass_critical", "T_reducing",
    "T_critical", "T_triple", "T_max", "T_min", "P_min", "P_max", "p_critical", "p_reducing", "p_triple",
    "fraction_min", "fraction_max", "T_freeze", "speed_of_sound", "viscosity", "conductivity", "surface_tension",
    "Prandtl", "isothermal_compressibility", "isobaric_expansion_coefficient", "isentropic_expansion_coefficient", "Z",
    "fundamental_derivative_of_gas_dynamics", "PIP", "alphar", "dalphar_dtau_constdelta", "dalphar_ddelta_consttau",
    "alpha0", "dalpha0_dtau_constdelta", "dalpha0_ddelta_consttau", "d2alpha0_ddelta2_consttau",
    "d3alpha0_ddelta3_consttau", "Phase",
]
# Since M10.3 the derivative strings (map 01 §4a; `CoolProp-Tests.cpp:1744, 1829`): the `eos` kind's 12 first and 10
# second partials, as CoolProp parses them from `PropsSI`'s output key, at the DT states, as the `eos` kind checks them
# (a heavy fluid's saturated liquid near its triple point, p ~ 1e-12 rho R T, leaves both sides' Jacobian ratios O(1)
# apart).
PROPS_DERIVATIVES = [f"d({of})/d({wrt})|{at}" for of, wrt, at in EOS_PARTIALS] + [
    f"d(d({of})/d({wrt})|{at})/d({wrt_2})|{at_2}" for of, wrt, at, wrt_2, at_2 in EOS_SECONDS
]
PROPS_TRIVIAL = [
    "GWP20", "GWP100", "GWP500", "FH", "HH", "PH", "ODP", "gas_constant", "molar_mass", "acentric", "dipole_moment",
    "rhomass_reducing", "rhomolar_reducing", "rhomolar_critical", "rhomass_critical", "T_reducing", "T_critical",
    "T_triple", "T_max", "T_min", "P_min", "P_max", "p_critical", "p_reducing", "p_triple", "fraction_min",
    "fraction_max", "T_freeze",
]
PROPS_COLUMNS = ["pair", "x1", "x2", "output", "status", "value"]
PROPS_UNITS = ["-", "pair", "pair", "-", "-", "output"]
PROPS_TOL = ["label", "in", "in", "label", "label", "prop"]
PROPS_STATES = {"core": 10, "full": 200}  # states per fluid, half DT and half QT


def props_dt_state(CP, name, rng, bounds, rho_l):
    """A DT state (pair, rho, T) whose CoolProp p is at most pmax, the next draw of `rng` that gives one."""
    t_low, t_max, p_max = bounds
    for _ in range(1000):
        t = uniform(rng, t_low, t_max)
        rho = log_uniform(rng, EOS_RHO_SPAN * rho_l, rho_l)
        p, status = call(CP, "PropsSI", ("P", "T", t, "Dmolar", rho, name))
        if status == "ok" and p <= p_max:
            return ("DT", rho, t)
    fail(f"props: {name}: no DT state at or below pmax in 1000 draws")


def props_rows(job):
    """One fluid's rows, in a pool child: (lines, floats)."""
    import CoolProp.CoolProp as CP  # the parent's module, inherited through fork

    name, states = job
    lines, floats = [], []
    for pair, x1, x2 in states:
        inputs = ("T", x2, "Dmolar", x1) if pair == "DT" else ("Q", x1, "T", x2)
        for output in PROPS_OUTPUTS + (PROPS_DERIVATIVES if pair == "DT" else []):
            value, status = call(CP, "PropsSI", (output, *inputs, name))
            floats += [x1, x2, value]
            lines.append(f"{pair},{cell(x1)},{cell(x2)},{output},{status},{cell(value)}\n")
    for output in PROPS_TRIVIAL:
        value, status = call(CP, "Props1SI", (output, name))
        floats += [math.nan, math.nan, value]
        lines.append(f"Props1SI,nan,nan,{output},{status},{cell(value)}\n")
    return lines, floats


def props(CP, lock, config, files, args):
    """The `props` kind: props/<Fluid>.csv for the core subset (10 states each) and the full set's files (200); no
    all-fluid tier."""
    if args.tier == "all":
        fail("--kind props has no all-fluid tier (section 3.5)")
    rows = args.rows or PROPS_STATES[args.tier]
    jobs, shas = [], {}
    for name in tier_fluids(args, files):
        shas[name] = assert_fluid(CP, files, name)
        state = CP.AbstractState("HEOS", name)
        t_triple = json.loads(files[name].read_text(encoding="utf-8"))["EOS"][0]["Ttriple"]
        t_low, t_max, tc, p_max = max(state.Tmin(), t_triple), state.Tmax(), state.T_critical(), state.pmax()
        state.update(CP.QT_INPUTS, 0, t_low)
        rho_l = state.rhomolar()
        rng = splitmix64(args.seed)
        states = []
        for i in range(rows):
            if i < rows // 2:
                states.append(props_dt_state(CP, name, rng, (t_low, t_max, p_max), rho_l))
            else:
                states.append(("QT", (0.0, 0.5, 1.0)[i % 3], uniform(rng, t_low, t_low + 0.9 * (tc - t_low))))
        jobs.append((name, states))
    grid = (f"n={rows} per fluid: DT T~U[Tlow,Tmax] rho~logU[{EOS_RHO_SPAN!r}*rhoL(Tlow),rhoL(Tlow)] drawn again "
            f"while p > pmax, then QT T~U[Tlow,Tlow+0.9(Tc-Tlow)] Q=0,0.5,1 cycling; Tlow=max(Tmin,EOS Ttriple); "
            f"seed={args.seed}; no phase imposed; the 85 outputs at each, then 22 derivative strings at the DT ones; then Props1SI "
            "of the 28 trivial outputs")
    out = {}
    for (name, _), (lines, floats) in zip(jobs, pool_map(props_rows, jobs, args.jobs)):
        text = header("props", lock, config, [(name, shas[name])], PROPS_COLUMNS, PROPS_TOL, floats,
                      units=PROPS_UNITS, grid=grid)
        out[f"props/{name}.csv"] = text + "".join(lines)
    return out


# The `codes` kind (section 3.5; map 01 R1, R2; D11): the v8.0.0 integer codes the Tier A shim pins. `codes/coolprop.csv`
# is the wheel's: every name `get_parameter_index` takes (the 85 outputs and the 24 aliases of `DataStructures.cpp:130-153`,
# each also in upper case, which `index_map_insert` registers too; `:157-160`) and a few it refuses, as -1; the phase names
# `get_phase_index` takes; the input-pair enum's values (`CoolProp.constants`). The wheel has no `get_input_pair_index`,
# so `codes/pair-names.csv` holds the names it takes, `input_pair_list` of `src/DataStructures.cpp` in order, collisions
# included: the map keeps a name's first entry (`index_map.emplace`, `:563-571`). Fluids and tiers play no part.
CODES_ALIASES = ["D", "H", "M", "S", "U", "C", "O", "G", "V", "L", "pcrit", "Pcrit", "Tcrit", "Ttriple", "ptriple",
                 "rhocrit", "Tmin", "Tmax", "pmax", "pmin", "molemass", "molarmass", "A", "I"]
CODES_REFUSED = ["dmass", "hMass", "Tcrit2", "D&H", "d(P)/d(T)|Dmolar"]
CODES_PHASES = ["phase_liquid", "phase_supercritical", "phase_supercritical_gas", "phase_supercritical_liquid",
                "phase_critical_point", "phase_gas", "phase_twophase", "phase_unknown", "phase_not_imposed"]
PAIR_SOURCE = "src/DataStructures.cpp"


def codes(CP, lock, config, files, args):
    import CoolProp.constants as constants

    def index(lookup, name):
        try:
            return int(lookup(name))
        except ValueError:
            return -1

    names = []
    for name in PROPS_OUTPUTS + CODES_ALIASES:
        names.extend(spelling for spelling in dict.fromkeys([name, name.upper()]) if spelling not in names)
    pairs = sorted((int(getattr(constants, n)), n) for n in dir(constants) if n.endswith("_INPUTS") or n == "INPUT_PAIR_INVALID")
    rows = [("parameter", name, index(CP.get_parameter_index, name)) for name in names + CODES_REFUSED]
    rows += [("phase", name, index(CP.get_phase_index, name)) for name in CODES_PHASES]
    rows += [("pair", name, value) for value, name in pairs]
    floats = [float(code) for _, _, code in rows]
    text = header("codes", lock, config, "none", ["table", "name", "index"], ["label", "label", "exact"], floats)
    wheel = text + "".join(f"{table},{name},{cell(float(code))}\n" for table, name, code in rows)
    source = (ROOT / "reference" / "CoolProp" / PAIR_SOURCE).read_text(encoding="utf-8")
    table = source.split("const input_pair_info input_pair_list[] = {", 1)[1].split("};", 1)[0]
    entries = re.findall(r'\{(\w+_INPUTS), "(\w+)",', table)
    if len(entries) != len(pairs) - 1:
        fail(f"{PAIR_SOURCE}: {len(entries)} input_pair_list entries, the wheel has {len(pairs) - 1} pairs")
    names_header = "".join(f"# {line}\n" for line in [
        "fixture: codes/v1", f"source: coolprop-source:{PAIR_SOURCE}@{lock['git']}", "columns: name,pair",
        "tol: label,label"])
    pair_names = names_header + "".join(f"{name},{pair}\n" for pair, name in entries)
    return {"codes/coolprop.csv": wheel, "codes/pair-names.csv": pair_names}


GENERATORS = {"facts": facts, "checkpoints": checkpoints, "crit": crit, "term": term, "eos": eos, "cubic": cubic,
              "cubic_sat": cubic_sat, "mix_alphar": mix_alphar, "mix_eos": mix_eos, "mix_crit": mix_crit, "mix_flash": mix_flash, "sat": sat, "flash": flash, "sigma": sigma, "melt": melt, "transport": transport, "refstate": refstate,
              "props": props, "codes": codes}


def write(out, files):
    """Each file to a temp name in its directory, then renamed."""
    for rel, text in sorted(files.items()):
        path = Path(out) / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        os.replace(tmp, path)


def write_manifest(out, files):
    """--write-manifest: rewrite the MANIFEST.sha256 lines of the files written ("<sha256> <bytes> <rows> <path>",
    path relative to the fixtures directory, section 3.4); other lines are kept, entries stay sorted by path."""
    entries, comments = {}, []
    for line in MANIFEST.read_text(encoding="utf-8").splitlines() if MANIFEST.exists() else []:
        if line.startswith("#"):
            comments.append(line)
        elif line:
            entries[line.rsplit(" ", 1)[1]] = line
    for rel, text in files.items():
        path = (Path(out) / rel).resolve()
        if not path.is_relative_to(FIXTURES_DIR.resolve()):
            fail(f"--write-manifest: {path} is not under {FIXTURES_DIR}")
        name = path.relative_to(FIXTURES_DIR.resolve()).as_posix()
        data = text.encode("utf-8")
        rows = sum(1 for line in text.splitlines() if not line.startswith("#"))
        entries[name] = f"{hashlib.sha256(data).hexdigest()} {len(data)} {rows} {name}"
    lines = comments + [entries[name] for name in sorted(entries)]
    write(MANIFEST.parent, {MANIFEST.name: "".join(f"{line}\n" for line in lines)})


def check(out, files):
    """--check: compare with what DIR holds, byte for byte."""
    differ = [rel for rel, text in sorted(files.items()) if not (Path(out) / rel).is_file()
              or (Path(out) / rel).read_bytes() != text.encode("utf-8")]
    for rel in differ:
        print(f"gen.py: {Path(out) / rel} differs from a fresh generation", file=sys.stderr)
    return 1 if differ else 0


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lock", required=True)
    parser.add_argument("--kind", required=True, choices=KINDS)
    parser.add_argument("--set", default="smoke")
    parser.add_argument("--tier", choices=["core", "all", "full"], default="core")
    parser.add_argument("--fluids")
    parser.add_argument("--rows", type=int)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--jobs", type=int, default=0)
    parser.add_argument("--out", required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-manifest", action="store_true")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    assert_environment()
    lock = read_lock(args.lock)
    generate = GENERATORS.get(args.kind)
    if generate is None:
        fail(f"--kind {args.kind} is added by the PLAN.md step that first uses it")
    if Path(args.out).resolve().is_relative_to(ROOT / "reference"):
        fail("nothing is written under reference/")
    CP = import_coolprop(lock)
    config = set_config(CP, lock)
    assert_golden()
    files = generate(CP, lock, config, fluid_files(lock), args)
    if args.check:
        return check(args.out, files)
    write(args.out, files)
    if args.write_manifest:
        write_manifest(args.out, files)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
