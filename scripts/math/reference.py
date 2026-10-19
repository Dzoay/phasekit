"""Reference values for core's in-house exp, ln, pow and expm1 (PLAN.md M23.1; docs/VERIFICATION.md section 3.4).

    LC_ALL=C uv run --no-project --python 3.12 --with mpmath==1.3.0 python -I scripts/math/reference.py \\
      --out crates/phasekit-verify/fixtures [--check] [--write-manifest]

Each value is evaluated by mpmath at 256 bits and stored as its correctly rounded double (`want`, ties to even) and the
residual `tail` = (exact - want) / ulp(want) to four significant digits, where ulp(want) is the spacing of doubles in
want's binade (2^-1074 in the subnormals and at 0): Arm's convention (optimized-routines math/test/ulp.h), in which its
error bounds are stated (user decision IM3). A value within 2^-100 ulp of a tie between two doubles stops the script.
Special values follow C99 Annex F, with residual 0.

- `math/<f>.csv`, about 4096 inputs each, labelled by `set`: `edge`, VERIFICATION.md section 9.2's edges and the
  thresholds of the libm crate's and Arm's code paths, each with its neighbours one ulp either side; `eos`, the
  arguments the EOS evaluates, from the 136 fluids of reference/CoolProp (exp's -60 to 5, ln's tau and delta, pow's
  bases and exponents from the fluids' terms and ancillaries, expm1's -theta*tau of the Planck-Einstein terms);
  `binade`, values uniform over every exponent of the function's domain, with a uniform mantissa.
- `math/arm/<f>.csv`: the inputs of Arm's directed test cases (math/test/testcases/directed/<f>.tst at the pin of
  scripts/fetch-optimized-routines.sh; `log.tst` for ln), evaluated as above. Each value is checked against Arm's
  result, which is its magnitude truncated, with 12 bits more where Arm gives them: the same double after rounding where
  those bits decide it (the result itself where Arm gives none), the exact values within 1/4096 of an ulp, and NaN where
  Arm's is NaN. Cases with a signalling NaN operand are left out.

Inputs are drawn with SplitMix64 (gen.py's), seed 1 for exp, 2 for ln, 3 for pow and 4 for expm1. Nothing is written
under reference/. Files are written to a temp name and renamed; `--check` compares with DIR.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import mpmath
from mpmath import mp, mpf

ROOT = Path(__file__).resolve().parents[2]
FLUIDS_DIR = ROOT / "reference" / "CoolProp" / "dev" / "fluids"
FLUIDS_LOCK = ROOT / "data" / "fluids.lock"
ARM_DIR = ROOT / "reference" / "optimized-routines"
ARM_SHA = "4be260a5117480382690c6d8c300bc784e927d76"  # v26.07, scripts/fetch-optimized-routines.sh
FIXTURES_DIR = ROOT / "crates" / "phasekit-verify" / "fixtures"
MANIFEST = FIXTURES_DIR / "MANIFEST.sha256"
MPMATH = "1.3.0"
PRECISION = 256
FUNCTIONS = ["exp", "ln", "pow", "expm1"]
ARM_FILES = {"exp": "exp.tst", "ln": "log.tst", "pow": "pow.tst", "expm1": "expm1.tst"}
SEEDS = {"exp": 1, "ln": 2, "pow": 3, "expm1": 4}
EOS_ROWS = 1950
BINADE_ROWS = 1950
MAX = sys.float_info.max
INF = math.inf
NAN = math.nan
LN2 = math.log(2)


def fail(message):
    print(f"reference.py: {message}", file=sys.stderr)
    sys.exit(1)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def assert_environment():
    """LC_ALL=C, mpmath at its version, the fluid files fluids.lock's and Arm's checkout at its pin."""
    if os.environ.get("LC_ALL") != "C":
        fail("LC_ALL must be C")
    if mpmath.__version__ != MPMATH:
        fail(f"mpmath {mpmath.__version__} is not {MPMATH}")
    for line in FLUIDS_LOCK.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            name, digest = line.split()
            if not (FLUIDS_DIR / name).is_file() or sha256(FLUIDS_DIR / name) != digest:
                fail(f"{FLUIDS_DIR / name} is missing or not fluids.lock's: run scripts/fetch-coolprop.sh")
    head = subprocess.run(["git", "-C", str(ARM_DIR), "rev-parse", "HEAD"], capture_output=True, text=True)
    if head.stdout.strip() != ARM_SHA:
        fail(f"{ARM_DIR} is not at {ARM_SHA}: run scripts/fetch-optimized-routines.sh")


# --- doubles --------------------------------------------------------------------------------------------------------


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def double(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def neighbours(x):
    """x and the doubles one ulp either side of it (finite x only)."""
    if not math.isfinite(x):
        return [x]
    return [x, math.nextafter(x, -INF), math.nextafter(x, INF)]


def signed(values):
    return [s * v for v in values for s in (1.0, -1.0)]


def splitmix64(seed):
    mask = (1 << 64) - 1
    state = seed
    while True:
        state = (state + 0x9E3779B97F4A7C15) & mask
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & mask
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & mask
        yield z ^ (z >> 31)


class Draw:
    """Uniform draws from SplitMix64: u in [0, 1) from the top 53 bits."""

    def __init__(self, seed):
        self.rng = splitmix64(seed)

    def u(self):
        return (next(self.rng) >> 11) * 2.0**-53

    def uniform(self, lo, hi):
        return lo + (hi - lo) * self.u()

    def log_uniform(self, lo, hi):
        return math.exp(self.uniform(math.log(lo), math.log(hi)))

    def integer(self, lo, hi):
        """An integer in [lo, hi]."""
        return lo + next(self.rng) % (hi - lo + 1)

    def binade(self, lo, hi):
        """(1 + u) * 2^e, e uniform in [lo, hi]; below 2^-1022 rounded to the subnormals' spacing."""
        return math.ldexp(1.0 + self.u(), self.integer(lo, hi))

    def sign(self):
        return -1.0 if next(self.rng) & 1 else 1.0

    def pick(self, items):
        return items[next(self.rng) % len(items)]


def ulp(x):
    """The spacing of doubles in finite x's binade: 2^-1074 in the subnormals and at 0."""
    return math.ldexp(1.0, max(math.frexp(x)[1] - 1, -1022) - 52) if x else 2.0**-1074


def nearest(v):
    """(want, tail): the double nearest the mpf v, ties to even, and (v - want) / ulp(want) as a float."""
    if v == 0:
        return 0.0, 0.0
    sign = -1.0 if v < 0 else 1.0
    a = abs(v)
    e = int(mpmath.frexp(a)[1]) - 1  # a in [2^e, 2^(e+1))
    q = max(e, -1022) - 52  # the exponent of a's ulp
    n = mpmath.ldexp(a, -q)
    k = int(mpmath.floor(n))
    frac = n - k
    if frac != 0.5 and abs(frac - 0.5) < mpf(2) ** -100:
        fail(f"{mpmath.nstr(v, 30)} lies within 2^-100 ulp of a tie")
    if frac > 0.5 or (frac == 0.5 and k % 2 == 1):
        k += 1
    try:
        want = math.ldexp(k, q)  # exact: k <= 2^53
    except OverflowError:
        return sign * INF, 0.0
    tail = (a - mpf(want)) / mpf(ulp(want))
    return sign * want, 0.0 if abs(tail) < mpf(2) ** -100 else sign * float(tail)


# --- the functions --------------------------------------------------------------------------------------------------


def is_integer(y):
    return math.isfinite(y) and y == math.floor(y)


def is_odd_integer(y):
    return is_integer(y) and abs(y) < 2.0**53 and int(y) % 2 == 1


def pow_special(x, y):
    """pow's value by C99 Annex F F.10.4.4 where it is one, else None."""
    if y == 0 or x == 1:
        return 1.0
    if math.isnan(x) or math.isnan(y):
        return NAN
    if x == 0:
        odd = is_odd_integer(y)
        if y < 0:
            return math.copysign(INF, x) if odd else INF
        return x if odd else 0.0
    if math.isinf(y):
        if x == -1:
            return 1.0
        return (INF if abs(x) < 1 else 0.0) if y < 0 else (0.0 if abs(x) < 1 else INF)
    if math.isinf(x):
        odd = is_odd_integer(y)
        if x < 0:
            return (-0.0 if odd else 0.0) if y < 0 else (-INF if odd else INF)
        return 0.0 if y < 0 else INF
    if x < 0 and not is_integer(y):
        return NAN
    return None


def special(f, x, y=None):
    """The value of f where Annex F gives it exactly, else None."""
    if f == "pow":
        return pow_special(x, y)
    if math.isnan(x):
        return NAN
    if f == "exp":
        return INF if x == INF else 0.0 if x == -INF else None
    if f == "expm1":
        return INF if x == INF else -1.0 if x == -INF else x if x == 0 else None
    # ln
    if x == 0:
        return -INF
    if x < 0:
        return NAN
    return INF if x == INF else None


def exact(f, x, y=None):
    """f at (x, y) as an mpf at 256 bits."""
    if f == "exp":
        return mp.exp(mpf(x))
    if f == "ln":
        return mp.log(mpf(x))
    if f == "expm1":
        return mp.expm1(mpf(x))
    magnitude = mp.power(abs(mpf(x)), mpf(y))
    return -magnitude if x < 0 and is_odd_integer(y) else magnitude


def evaluate(f, x, y=None):
    """(want, tail) of f at (x, y)."""
    value = special(f, x, y)
    if value is not None:
        return value, 0.0
    return nearest(exact(f, x, y))


# --- the fluids' arguments ------------------------------------------------------------------------------------------


def non_integer(values):
    return {float(v) for v in values if not float(v).is_integer()}


def fluid_arguments():
    """tau's and delta's ranges over the 136 fluids, and the exponents pow takes and the theta_k expm1's terms carry."""
    taus, deltas, theta_hi = [], [], 0.0
    exponents = {"tau": set(), "delta": set(), "theta": set(), "Delta": set(), "delta1": set()}
    planck = set()
    for path in sorted(FLUIDS_DIR.glob("*.json")):
        fluid = json.loads(path.read_text(encoding="utf-8"))
        eos = fluid["EOS"][0]
        states = eos["STATES"]
        t_r, rho_r = states["reducing"]["T"], states["reducing"]["rhomolar"]
        taus += [t_r / eos["T_max"], t_r / eos["Ttriple"]]
        deltas.append(states["sat_min_liquid"]["rhomolar"] / rho_r)
        theta_hi = max(theta_hi, 1 - eos["Ttriple"] / t_r)
        for term in eos["alphar"]:
            kind = term["type"]
            exponents["tau"] |= non_integer(term.get("t", []))
            exponents["tau"] |= non_integer(term.get("m", []))
            for key in ("d", "l", "ld"):
                exponents["delta"] |= non_integer(term.get(key, []))
            if kind == "ResidualHelmholtzNonAnalytic":
                exponents["Delta"] |= non_integer(term["b"])
                exponents["delta1"] |= non_integer(term["a"]) | {1 / (2 * b) for b in term["beta"]}
        for term in eos["alpha0"]:
            kind = term["type"]
            if kind == "IdealGasHelmholtzPower":
                exponents["tau"] |= non_integer(term["t"])
            elif kind == "IdealGasHelmholtzCP0PolyT":
                exponents["tau"] |= {-t for t in non_integer(term["t"])}
            elif kind in ("IdealGasHelmholtzPlanckEinstein", "IdealGasHelmholtzPlanckEinsteinGeneralized"):
                planck |= {float(t) for t in term["t"]}
            elif kind == "IdealGasHelmholtzPlanckEinsteinFunctionT":
                planck |= {float(v) / term["Tcrit"] for v in term["v"]}
        for name in ("pS", "rhoL", "rhoV"):
            ancillary = fluid.get("ANCILLARIES", {}).get(name)
            if ancillary:
                exponents["theta"] |= non_integer(ancillary["t"])
    tau = (min(taus), max(taus))
    delta = (1e-12, max(deltas))
    ranges = {"tau": tau, "delta": delta, "theta": (1e-6, theta_hi), "Delta": (1e-10, 10.0),
              "delta1": (1e-16, (delta[1] - 1) ** 2)}
    pools = {kind: sorted(values) for kind, values in exponents.items() if values}
    return ranges, pools, sorted(planck)


# --- the inputs -----------------------------------------------------------------------------------------------------


def exp_inputs(draw, ranges, pools, planck):
    edges = [0.0, 2.0**-54, 2.0**-28, double(0x3E300000FFFFFFFF), 0.5 * LN2, double(0x3FD62E42FFFFFFFF),
             1.5 * LN2, double(0x3FF0A2B200000000), 1.0, 512.0, double(0x4086232B00000000),
             double(0x4086232BDD7ABCD2), -math.log(2.0**-1022), double(0x40862E42FEFA39EF), 745.13321910194110842,
             1024.0, 1e-300, 5e-324, MAX, INF, NAN, 708.5, 720.0, 740.0, 745.0, 1000.0]
    edges = [v for e in signed(edges) for v in neighbours(e)]
    eos = [draw.uniform(-60.0, 5.0) for _ in range(EOS_ROWS)]
    binade = [draw.sign() * draw.binade(-60, 9) for _ in range(BINADE_ROWS)]
    return edges, eos, binade


def ln_inputs(draw, ranges, pools, planck):
    edges = [1.0, 2.0, 0.5, math.e, 10.0, double(0x3FE6A09E667F3BCD), double(0x3FEE000000000000),
             double(0x3FF1090000000000), sys.float_info.min, 5e-324, 1e-300, MAX, INF, NAN, 0.0, -1.0, -INF]
    edges = [v for e in edges + [-0.0] for v in neighbours(e)]
    eos = []
    for i in range(EOS_ROWS):
        lo, hi = ranges["tau"] if i % 2 == 0 else ranges["delta"]
        eos.append(draw.log_uniform(lo, hi))
    binade = [draw.binade(-1074, 1023) for _ in range(BINADE_ROWS)]
    return edges, eos, binade


def pow_inputs(draw, ranges, pools, planck):
    edges = [(1.0, NAN), (NAN, 0.0), (2.0, 1023.0), (2.0, 1024.0), (2.0, -1022.0), (2.0, -1074.0), (2.0, -1075.0),
             (2.0, -1074.5), (0.5, 1075.0), (10.0, 308.0), (10.0, -323.0), (-2.0, 3.0), (-2.0, 0.5), (-8.0, 1 / 3),
             (MAX, math.nextafter(1.0, 2.0)), (4.0, 0.5), (9.0, 0.5), (5e-324, 0.5), (0.0, -1.0), (-0.0, -1.0),
             (-0.0, -2.0), (-1.0, INF), (0.5, -INF), (2.0, INF), (-INF, -3.0), (INF, 0.5), (math.e, 1.0),
             (math.nextafter(1.0, 2.0), 2.0**62), (math.nextafter(1.0, 0.0), -(2.0**62)), (1.5, 1750.0),
             (1.5, -1840.0)]
    edges = [(x, y) for x0, y0 in edges for x in neighbours(x0) for y in neighbours(y0)]
    kinds = sorted(pools)
    eos = []
    for _ in range(EOS_ROWS):
        kind = draw.pick(kinds)
        eos.append((draw.log_uniform(*ranges[kind]), draw.pick(pools[kind])))
    binade = []
    while len(binade) < BINADE_ROWS:
        x = draw.binade(-1074, 1023)
        if x != 1.0:
            binade.append((x, draw.uniform(-1074.0, 1024.0) / math.log2(x)))
    return edges, eos, binade


def expm1_inputs(draw, ranges, pools, planck):
    edges = [0.0, 2.0**-54, 2.0**-28, 0.5 * LN2, 1.5 * LN2, double(0x3FD62E42FFFFFFFF), double(0x3FF0A2B200000000),
             56 * LN2, double(0x40862E42FEFA39EF), 1e-20, 0.1, 1.0, 1e-300, 5e-324, MAX, INF, NAN]
    edges = [v for e in signed(edges) for v in neighbours(e)]
    eos = [-draw.pick(planck) * draw.log_uniform(*ranges["tau"]) for _ in range(EOS_ROWS)]
    binade = [draw.sign() * draw.binade(-1074, 9) for _ in range(BINADE_ROWS)]
    return edges, eos, binade


INPUTS = {"exp": exp_inputs, "ln": ln_inputs, "pow": pow_inputs, "expm1": expm1_inputs}


# --- Arm's directed cases -------------------------------------------------------------------------------------------


def signalling(word):
    """Whether the bits `word` are a signalling NaN."""
    return word & 0x7FF0000000000000 == 0x7FF0000000000000 and word & 0x000FFFFFFFFFFFFF and not word & (1 << 51)


def arm_cases(f):
    """(inputs, result bits, extra digits or None) of each case of Arm's file for f, but those with a signalling NaN
    operand: the format writes every NaN as `nan`, and Rust keeps no NaN signalling, so pow(1, sNaN), NaN in Arm's
    file, would read as pow(1, NaN), which is 1."""
    path = ARM_DIR / "math" / "test" / "testcases" / "directed" / ARM_FILES[f]
    cases = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("func="):
            continue
        fields = dict(field.split("=", 1) for field in line.split())
        words = [int(fields[key].replace(".", ""), 16) for key in ("op1", "op2") if key in fields]
        if any(signalling(word) for word in words):
            continue
        ops = [double(word) for word in words]
        parts = fields["result"].split(".")
        cases.append((tuple(ops), int(parts[0] + parts[1], 16), int(parts[2], 16) if len(parts) > 2 else None))
    return path, cases


def check_arm(f, inputs, want, tail, result, extra):
    """Our value agrees with Arm's result `result` (its magnitude truncated) and extra digits `extra` (12 bits)."""
    got = double(result)
    where = f"{f}{inputs}: ours {want!r} {tail!r}, Arm's {result:016x}" + (f".{extra:03x}" if extra is not None else "")
    if math.isnan(got) or math.isnan(want):
        if not (math.isnan(got) and math.isnan(want)):
            fail(f"NaN differs: {where}")
        return
    if extra is None or extra < 0x800:
        ok = bits(want) == result
    elif extra > 0x800:
        ok = bits(want) == result + 1
    else:
        ok = bits(want) in (result, result + 1)
    if ok and extra is not None and math.isfinite(want):
        # Both exact values, |result| + extra/4096 of its ulp and want + tail of want's, agree to Arm's 12 bits. A case
        # without extra digits states only the rounded result.
        theirs = mpf(abs(got)) + mpf(extra or 0) / 4096 * mpf(ulp(got))
        mine = abs(mpf(want) + mpf(tail) * mpf(ulp(want)))
        ok = abs(mine - theirs) <= mpf(ulp(got)) * (mpf(1) / 4096 + mpf(1e-6))
    if not ok:
        fail(f"Arm's case disagrees: {where}")


# --- the files ------------------------------------------------------------------------------------------------------


def cell(value):
    """Python repr, the shortest round trip; NaN as nan."""
    return "nan" if math.isnan(value) else repr(float(value))


def fnv1a64(values):
    """FNV-1a 64 over the little-endian bits of every float cell, NaN as 0x7ff8000000000000 (section 3.3)."""
    h = 0xCBF29CE484222325
    for value in values:
        data = struct.pack("<Q", 0x7FF8000000000000) if math.isnan(value) else struct.pack("<d", value)
        for byte in data:
            h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def generator_line():
    script = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    return f"reference.py sha256={script} mpmath={MPMATH} prec={PRECISION} python={platform.python_version()}"


def fixture(f, rows, grid=None, fluid=None):
    """The `math/v1` text (section 3.3): rows of (inputs, want, tail[, set])."""
    inputs = ["x", "y"] if f == "pow" else ["x"]
    labelled = grid is not None
    columns = [*inputs, "want", "tail", *(["set"] if labelled else [])]
    tol = [*("in" for _ in inputs), "faithful", "in", *(["label"] if labelled else [])]
    units = [*("-" for _ in inputs), "-", "ulp", *(["-"] if labelled else [])]
    floats = [v for row in rows for v in row[: len(inputs) + 2]]
    lines = [
        "fixture: math/v1",
        f"generator: {generator_line()}",
        "env: LC_ALL=C",
        *([f"fluid: {fluid}"] if fluid else []),
        *([f"grid: {grid}"] if grid else []),
        f"source: mp:mpmath-{MPMATH}",
        "columns: " + ",".join(columns),
        "units: " + ",".join(units),
        "tol: " + ",".join(tol),
        f"bits: fnv1a64={fnv1a64(floats)}",
    ]
    body = "".join(",".join(v if isinstance(v, str) else cell(v) for v in row) + "\n" for row in rows)
    return "".join(f"# {line}\n" for line in lines) + body


def residual(tail):
    """A residual as written: four significant digits, enough for an error measured in ulps."""
    return float(f"{tail:.4g}")


def key(inputs):
    return tuple(0x7FF8000000000000 if math.isnan(v) else bits(v) for v in inputs)


def ours(f, ranges, pools, planck):
    """math/<f>.csv's text."""
    edges, eos, binade = INPUTS[f](Draw(SEEDS[f]), ranges, pools, planck)
    rows, seen = [], set()
    for label, values in (("edge", edges), ("eos", eos), ("binade", binade)):
        for value in values:
            inputs = value if isinstance(value, tuple) else (value,)
            if key(inputs) in seen:
                continue
            seen.add(key(inputs))
            want, tail = evaluate(f, *inputs)
            rows.append((*inputs, want, residual(tail), label))
    lo, hi = ranges["tau"]
    dlo, dhi = ranges["delta"]
    grid = (f"seed={SEEDS[f]} edge=edges and thresholds +-1 ulp eos={EOS_ROWS} binade={BINADE_ROWS} "
            f"fluids=136 (data/fluids.lock) tau=[{lo:.6g},{hi:.6g}] delta=[{dlo:.6g},{dhi:.6g}]")
    return fixture(f, rows, grid=grid)


def arm(f):
    """math/arm/<f>.csv's text."""
    path, cases = arm_cases(f)
    rows, seen = [], set()
    for inputs, result, extra in cases:
        want, tail = evaluate(f, *inputs)
        check_arm(f, inputs, want, tail, result, extra)
        if key(inputs) not in seen:
            seen.add(key(inputs))
            rows.append((*inputs, want, residual(tail)))
    rel = path.relative_to(ARM_DIR).as_posix()
    return fixture(f, rows, fluid=f"optimized-routines v26.07 {rel} sha256={sha256(path)}")


def generate():
    mp.prec = PRECISION
    ranges, pools, planck = fluid_arguments()
    files = {}
    for f in FUNCTIONS:
        files[f"math/{f}.csv"] = ours(f, ranges, pools, planck)
        files[f"math/arm/{f}.csv"] = arm(f)
    return files


def write(out, files):
    """Each file to a temp name in its directory, then renamed."""
    for rel, text in sorted(files.items()):
        path = Path(out) / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        os.replace(tmp, path)


def write_manifest(out, files):
    """--write-manifest: rewrite the MANIFEST.sha256 lines of the files written, as gen.py does (section 3.4)."""
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
        print(f"reference.py: {Path(out) / rel} differs from a fresh generation", file=sys.stderr)
    return 1 if differ else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args(argv)
    assert_environment()
    if Path(args.out).resolve().is_relative_to(ROOT / "reference"):
        fail("nothing is written under reference/")
    files = generate()
    if args.check:
        return check(args.out, files)
    write(args.out, files)
    if args.write_manifest:
        write_manifest(args.out, files)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
