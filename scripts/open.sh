#!/usr/bin/env bash
# open.sh <step> <branch> <title> <mutants note>: fills the PR body's GATES from the local logs (prep.sh), adds the
# mutants row, pushes the branch and opens the PR against main; prints the PR URL. The body is $SP/pr-<step>.md.
SP=$(dirname "$(readlink -f "$0")"); ROOT=${PHASEKIT_ROOT:-$HOME/Projects}
step=$1; branch=$2; title=$3; note=$4
[ ${#title} -le 72 ] || { echo "title is ${#title} chars"; exit 1; }
cat $SP/gates-$step/G8-*.txt | grep -E "^gates [a-z-]+: ok" > $SP/gates-$step/G8.txt
python3 $SP/fill.py $SP/pr-$step.md $SP/gates-$step >/dev/null || exit 1
python3 - $SP/pr-$step.md "$note" <<'PY'
import sys
p, note = sys.argv[1], sys.argv[2]; s = open(p).read()
old = "| deny, shear, reuse | green |"
s = s.replace(old, f"| mutants | the local pre-check of this step's own diff, on main's settings: {note}; CI runs the gate |\n" + old, 1)
open(p, 'w').write(s)
PY
cd $ROOT/phasekit-$step && git push -q -u origin $branch --force-with-lease 2>&1 | grep -v "^remote:" | tail -1
cd $ROOT/phasekit && gh pr create --base main --head $branch --title "$title" --body-file $SP/pr-$step.md 2>&1 | tail -1
