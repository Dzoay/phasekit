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
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FLUIDS_DIR = ROOT / "reference" / "CoolProp" / "dev" / "fluids"
FIXTURES_DIR = ROOT / "crates" / "phasekit-verify" / "fixtures"
MANIFEST = FIXTURES_DIR / "MANIFEST.sha256"
KINDS = ["facts", "checkpoints", "term", "eos", "crit", "sat", "flash", "transport", "sigma", "melt", "refstate",
         "props", "codes"]
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


def call(CP, fn, args):
    """One oracle call -> (value, status). `PropsSI` raises on failure, but `Props1SI` returns inf and leaves the
    message in the process-wide errstring (CoolProp's C convention; reading errstring clears it): both are checked."""
    CP.get_global_param_string("errstring")
    try:
        value = float(abstract_state(CP, *args) if fn == "AbstractState" else getattr(CP, fn)(*args))
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
    name = {"PropsSI": inputs[-1], "Props1SI": inputs[0], "AbstractState": inputs[1]}[fn]
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
    """One row: a fresh `AbstractState` of `fluid` with the phase imposed at (T, rho); (floats, line)."""
    try:
        state = CP.AbstractState("HEOS", fluid)
        state.specify_phase(CP.iphase_gas)
        state.update(CP.DmolarT_INPUTS, rho, t)
        values, status = [state.tau(), state.delta(), *(getattr(state, m)() for m in TERM_METHODS)], "ok"
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
    near tau = delta = 1) in term/<Fluid>.csv; the all-fluid tier the alpha^r totals (`block_idx = all`) at the first 4
    points of every fluid in all/term.csv; the full tier 300 points per block and 64 totals per fluid."""
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


GENERATORS = {"facts": facts, "checkpoints": checkpoints, "crit": crit, "term": term}


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
