"""GERG-2008 fixtures (PLAN.md M13.5; docs/VERIFICATION.md section 3.8): teqp's vectors and AGA8's test data.

The second generator beside gen.py, run as `cargo xtask teqp` runs it, in the same pinned image:

    env -i PATH="$PATH" HOME="$HOME" LC_ALL=C TZ=UTC PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 \\
      <teqp.lock's uv_command> python scripts/oracle/teqp.py --lock crates/phasekit-verify/fixtures/teqp.lock \\
        --kind gerg --out crates/phasekit-verify/fixtures

Kinds:
- `aga8`: NIST's AGA8 code's GERG-2008 test data, read from the pinned checkout (scripts/fetch-aga8.sh): the check
  state of AGA8CODE/C/GERG2008_test_01.cpp (`aga8/check-state.csv`), the 3645 rows of TESTDATA/Test Data.xls's binary
  sheet (`aga8/binary.csv`, its outputs at their eight significant digits) and its 200 natural gases (`aga8/gases.csv`, mole
  fractions normalised to sum 1: AGA8 rounds each to 1e-6 mol%, so 23 of them sum to 100 +- 3e-6).
- `gerg`: teqp's GERG-2008 residual (`GERG2008resid`) for the 21 pure components and every binary at 50/50 at four
  states of each one's reducing state, and for AGA8's natural gases at two (`teqp-0.23.2/gerg/*.csv`): T_r, rho_r and
  alpha^r with its scaled derivatives A_ij to order 3. teqp's ideal gas is not used: it takes theta_k as GERG TM15's
  nine-decimal vartheta_k times T_c, which moves alpha0's tau-derivatives about 1e-11 from AGA8's theta_k in K.

Nothing is written under reference/. Files are written to a temp name and renamed; `--check` compares with DIR.
"""

import argparse
import hashlib
import math
import os
import platform
import re
import struct
import sys
import tempfile
from pathlib import Path

# This file is named after the package it drives: drop its own directory from the import path, or `import teqp` finds it.
sys.path = [p for p in sys.path if Path(p or ".").resolve() != Path(__file__).resolve().parent]

ROOT = Path(__file__).resolve().parents[2]
AGA8_DIR = ROOT / "reference" / "AGA8"
FIXTURES_DIR = ROOT / "crates" / "phasekit-verify" / "fixtures"
MANIFEST = FIXTURES_DIR / "MANIFEST.sha256"
KINDS = ["aga8", "gerg"]
CHECK_STATE = "AGA8CODE/C/GERG2008_test_01.cpp"
TEST_DATA = "TESTDATA/Test Data.xls"

# GERG-2008's 21 components in AGA8's order, as teqp names them.
TEQP_NAMES = ["methane", "nitrogen", "carbondioxide", "ethane", "propane", "isobutane", "n-butane", "isopentane",
              "n-pentane", "n-hexane", "n-heptane", "n-octane", "n-nonane", "n-decane", "hydrogen", "oxygen",
              "carbonmonoxide", "water", "hydrogensulfide", "helium", "argon"]
# AGA8's test-data spellings (Test Data.xls) of the same components, in the same order.
AGA8_NAMES = ["methane", "nitrogen", "CO2", "ethane", "propane", "ibutane", "butane", "ipentane", "pentane", "hexane",
              "heptane", "octane", "nonane", "decane", "hydrogen", "oxygen", "CO", "water", "H2S", "helium", "argon"]

# (tau, delta) of each mixture's own reducing state: dilute, the two states of mix_alphar, and a dense one.
PURE_STATES = [(0.6, 0.1), (0.8, 0.8), (1.25, 1.6), (1.5, 2.4)]
GAS_STATES = [(0.8, 0.8), (1.25, 1.6)]
ALPHAR = [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2), (3, 0), (2, 1), (1, 2), (0, 3)]
ALPHAR_COLUMNS = ["alphar", *(f"A{i}{j}" for i, j in ALPHAR[1:])]


def fail(message):
    print(f"teqp.py: {message}", file=sys.stderr)
    sys.exit(1)


def read_lock(path):
    """`key value` lines; `#` comments."""
    lock = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            key, _, value = line.partition(" ")
            lock[key] = value.strip()
    return lock


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def assert_environment(lock):
    """LC_ALL=C; the AGA8 checkout's files are the lock's."""
    if os.environ.get("LC_ALL") != "C":
        fail("LC_ALL must be C")
    for rel, key in [(CHECK_STATE, "aga8_check_sha256"), (TEST_DATA, "aga8_testdata_sha256")]:
        path = AGA8_DIR / rel
        if not path.is_file():
            fail(f"{path} is missing: run scripts/fetch-aga8.sh")
        if sha256(path) != lock[key]:
            fail(f"{rel}: sha256 {sha256(path)} is not teqp.lock's {lock[key]}")


def import_teqp(lock):
    """teqp at the lock's version, its extension module's sha256 the lock's."""
    import importlib.util

    import numpy
    import teqp

    if teqp.__version__ != lock["version"]:
        fail(f"teqp {teqp.__version__} is not {lock['version']}")
    if numpy.__version__ != lock["numpy"]:
        fail(f"numpy {numpy.__version__} is not {lock['numpy']}")
    so = Path(importlib.util.find_spec("teqp").origin).parent / lock["so_name"]
    if sha256(so) != lock["so_sha256"]:
        fail(f"{so}: sha256 {sha256(so)} is not teqp.lock's {lock['so_sha256']}")
    return teqp, numpy


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


def generator_line(lock):
    script = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    libc = "-".join(part for part in platform.libc_ver() if part) or "unknown"
    return (f"teqp.py sha256={script} teqp={lock['version']} so_sha256={lock['so_sha256']} numpy={lock['numpy']} "
            f"python={platform.python_version()} libc={libc}")


def fixture(kind, lock, fluid, source, columns, units, tol, rows, grid=None):
    """The `<kind>/v1` text (section 3.3): rows of cells, each a float or a label string."""
    floats = [v for row in rows for v, t in zip(row, tol) if t != "label"]
    lines = [
        f"fixture: {kind}/v1",
        f"generator: {generator_line(lock)}",
        "env: LC_ALL=C",
        f"fluid: {fluid}",
        *([f"grid: {grid}"] if grid else []),
        f"source: {source}",
        "columns: " + ",".join(columns),
        "units: " + ",".join(units),
        "tol: " + ",".join(tol),
        f"bits: fnv1a64={fnv1a64(float(v) for v in floats)}",
    ]
    body = "".join(",".join(v if t == "label" else printed(v) for v, t in zip(row, tol)) + "\n" for row in rows)
    return "".join(f"# {line}\n" for line in lines) + body


def printed(value):
    """A float cell: a str kept as read (AGA8's printed digits), a float as its repr."""
    return value if isinstance(value, str) else cell(value)


# --- aga8 ----------------------------------------------------------------------------------------------------------

CHECK_QUANTITIES = [("mm", "g/mol"), ("D", "mol/l"), ("P", "kPa"), ("Z", "-"), ("dPdD", "kPa/(mol/l)"),
                    ("d2PdD2", "kPa/(mol/l)^2"), ("dPdT", "kPa/K"), ("U", "J/mol"), ("H", "J/mol"), ("s", "J/(mol K)"),
                    ("Cv", "J/(mol K)"), ("Cp", "J/(mol K)"), ("W", "m/s"), ("G", "J/mol"), ("JT", "K/kPa"),
                    ("Kappa", "-")]


def check_state(lock):
    """GERG2008_test_01.cpp's composition, T, P and every `<name>_reference` it tests at 1e-8, each as its repr."""
    text = (AGA8_DIR / CHECK_STATE).read_text(encoding="latin-1")
    x = re.search(r"double _x\[\] = \{([^}]*)\}", text).group(1).replace("\n", " ").split(",")
    x = [token.strip() for token in x]
    t = re.search(r"double T = ([0-9.]+)", text).group(1)
    p = re.search(r"P = ([0-9.]+)", text).group(1)
    refs = []
    for name, _ in CHECK_QUANTITIES:
        refs.append(re.search(rf"double {name}_reference\s*=\s*([-+0-9.Ee]+);", text).group(1))
    if len(x) != 21:
        fail(f"{CHECK_STATE}: {len(x)} mole fractions, not 21")
    columns = [f"x{i}" for i in range(1, 22)] + ["T", "P_in"] + [name for name, _ in CHECK_QUANTITIES]
    units = ["-"] * 21 + ["K", "kPa"] + [unit for _, unit in CHECK_QUANTITIES]
    tol = ["in"] * 23 + ["prop"] * len(CHECK_QUANTITIES)
    tol[columns.index("D")] = "flash"
    row = [float(v) for v in (*x, t, p, *refs)]
    source = f"aga8:{CHECK_STATE}@{lock['aga8_git'][:8]}"
    fluid = f"{CHECK_STATE} sha256={lock['aga8_check_sha256']}"
    return fixture("aga8_check", lock, fluid, source, columns, units, tol, [row])


def workbook():
    import xlrd

    return xlrd.open_workbook(str(AGA8_DIR / TEST_DATA))


def binary_rows(book):
    """The binary sheet's GERG-2008 columns: pair, T, D, P, cv, cp, w, from row 12 on (3645 rows): T and D as stored,
    the four outputs at the eight significant digits AGA8 rounds them to, trailing zeros kept (the sheet stores
    doubles, which drop them; one, row 260's c_p, is stored as 89.36728600000001)."""
    sheet = book.sheet_by_name("Binary Mixture Calculations")
    rows = []
    for r in range(11, sheet.nrows):
        label = sheet.cell_value(r, 20).split(",")
        if len(label) != 4 or label[1] != "0.5" or label[3] != "0.5":
            fail(f"binary sheet row {r + 1}: `{sheet.cell_value(r, 20)}` is not an equimolar pair")
        values = [sheet.cell(r, c) for c in (2, 3, 5, 6, 7, 8)]
        if any(v.ctype != 2 for v in values):
            fail(f"binary sheet row {r + 1}: a cell is not a number")
        t, d, *outputs = (v.value for v in values)
        rows.append([f"{label[0]}&{label[2]}", cell(t), cell(d), *(format(v, "#.8g") for v in outputs)])
    if len(rows) != 3645:
        fail(f"binary sheet: {len(rows)} rows, not 3645")
    return rows


def gas_rows(book):
    """The 200 natural gases, by AGA8's gas number (its Excel row), in GERG's component order, normalised."""
    sheet = book.sheet_by_name("Natural Gas Compositions")
    header = [sheet.cell_value(0, c) for c in range(21)]
    order = [header.index(name) for name in AGA8_NAMES]
    rows = []
    for r in range(1, sheet.nrows):
        cells = [sheet.cell(r, c) for c in range(21)]
        if not all(c.ctype == 2 for c in cells):
            continue
        percent = [cells[c].value for c in order]
        total = math.fsum(percent)
        if abs(total - 100.0) > 1e-5:
            fail(f"gas {r + 1}: mole percents sum to {total}")
        rows.append([str(r + 1), *(p / total for p in percent)])
    if len(rows) != 200:
        fail(f"{len(rows)} natural gases, not 200")
    return rows


def aga8(lock, args):
    book = workbook()
    fluid = f"{TEST_DATA} sha256={lock['aga8_testdata_sha256']}"
    source = f"aga8:{TEST_DATA}@{lock['aga8_git'][:8]}"
    binary = fixture("aga8_binary", lock, fluid, source, ["pair", "T", "D", "P", "cv", "cp", "w"],
                     ["-", "K", "mol/l", "MPa", "J/(mol K)", "J/(mol K)", "m/s"],
                     ["label", "in", "in", "paper", "paper", "paper", "paper"], binary_rows(book))
    gases = fixture("aga8_gases", lock, fluid, source, ["gas", *TEQP_NAMES], ["-"] * 22, ["label", *["in"] * 21],
                    gas_rows(book))
    return {"aga8/check-state.csv": check_state(lock), "aga8/binary.csv": binary, "aga8/gases.csv": gases}


# --- gerg ----------------------------------------------------------------------------------------------------------


def teqp_rows(teqp, numpy, label, names, z, states):
    """teqp's GERG2008resid at each (tau, delta) of the mixture's reducing state."""
    model = teqp.make_model({"kind": "GERG2008resid", "model": {"names": names}})
    z = numpy.array(z)
    t_r, rho_r = model.get_Tr(z), model.get_rhor(z)
    rows = []
    for tau, delta in states:
        t, rho = t_r / tau, delta * rho_r
        rows.append([label, t, rho, t_r, rho_r, *(model.get_Arxy(i, j, t, rho, z) for i, j in ALPHAR)])
    return rows


def gerg(lock, args):
    teqp, numpy = import_teqp(lock)
    columns = ["T", "rhomolar", "Tr", "rhor", *ALPHAR_COLUMNS]
    units = ["K", "mol/m3", "K", "mol/m3", *["-"] * len(ALPHAR)]
    tol = ["in", "in", "prop", "prop", *["term"] * len(ALPHAR)]
    source, fluid = f"teqp:{lock['version']}", f"GERG2008resid ({len(TEQP_NAMES)} components)"
    grid = lambda states: "tau,delta of the mixture's T_r, rho_r in " + " ".join(f"({a},{b})" for a, b in states)
    pure = [row for name in TEQP_NAMES for row in teqp_rows(teqp, numpy, name, [name], [1.0], PURE_STATES)]
    pairs = [(a, b) for i, a in enumerate(TEQP_NAMES) for b in TEQP_NAMES[i + 1:]]
    binary = [row for a, b in pairs for row in teqp_rows(teqp, numpy, f"{a}&{b}", [a, b], [0.5, 0.5], PURE_STATES)]
    gas = [row for g in gas_rows(workbook())
           for row in teqp_rows(teqp, numpy, g[0], TEQP_NAMES, g[1:], GAS_STATES)]
    write = lambda kind, label, rows, states: fixture(kind, lock, fluid, source, [label, *columns],
                                                      ["-", *units], ["label", *tol], rows, grid(states))
    return {
        "teqp-0.23.2/gerg/pure.csv": write("gerg_pure", "component", pure, PURE_STATES),
        "teqp-0.23.2/gerg/binary.csv": write("gerg_binary", "pair", binary, PURE_STATES),
        "teqp-0.23.2/gerg/gas.csv": write("gerg_gas", "gas", gas, GAS_STATES),
    }


GENERATORS = {"aga8": aga8, "gerg": gerg}


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
        print(f"teqp.py: {Path(out) / rel} differs from a fresh generation", file=sys.stderr)
    return 1 if differ else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lock", required=True)
    parser.add_argument("--kind", required=True, choices=KINDS)
    parser.add_argument("--out", required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args(argv)
    lock = read_lock(args.lock)
    assert_environment(lock)
    if Path(args.out).resolve().is_relative_to(ROOT / "reference"):
        fail("nothing is written under reference/")
    files = GENERATORS[args.kind](lock, args)
    if args.check:
        return check(args.out, files)
    write(args.out, files)
    if args.write_manifest:
        write_manifest(args.out, files)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
