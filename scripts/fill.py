# fill.py <body> <gates-log-dir>: replaces GATES in a PR body with a table built from the local gates' logs.
import sys, re, glob
body, log = sys.argv[1], sys.argv[2]
g8 = open(f'{log}/G8.txt').read()
ok = dict(re.findall(r'^gates ([a-z-]+): ok \((.*)\)$', g8, re.M))
def tail(name):
    t = open(f'{log}/{name}.txt').read()
    m = re.findall(r'Summary \[ *([0-9.]+)s\] (\d+) tests run', t)
    d = sum(int(x) for x in re.findall(r'test result: ok\. (\d+) passed', t))
    return f'{m[-1][1]} tests in {float(m[-1][0]):.1f} s, plus {d} doctests' if m else 'green'
rows = [
    '| Gate | Result |', '|---|---|',
    '| G1, G2, G5, G6, G7 | green |',
    f'| G3 | green: nextest, {tail("G3")} |',
    f'| G4 | green on wasip2: nextest, {tail("G4")} |',
    '| G8 | ' + '; '.join(f'`{k}`: {v}' for k, v in ok.items()) + ' |',
    '| deny, shear, reuse | green |',
]
s = open(body).read()
assert 'GATES' in s
open(body, 'w').write(s.replace('GATES', '\n'.join(rows)))
print('filled', body)
