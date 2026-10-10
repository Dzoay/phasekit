#!/usr/bin/env bash
# prep.sh <step>: in $ROOT/phasekit-<step>, update ci/test-counts.txt and run the local gates without mutants; commits
# the counts as `chore: test counts` only if every gate passed. Logs: $SP/gates-<step>/, summary $SP/gates-<step>.out.
SP=$(dirname "$(readlink -f "$0")"); ROOT=${PHASEKIT_ROOT:-$HOME/Projects}
step=$1; w=$ROOT/phasekit-$step
export CARGO_TARGET_DIR=$HOME/.cache/phasekit-target-$step PHASEKIT_REFERENCE_DIR=$ROOT/phasekit/reference
cd $w || exit 2
cargo xtask gates counts --update 2>&1 | tail -1
TARGET=$CARGO_TARGET_DIR LOG=$SP/gates-$step G8_GATES="deps lints counts ignores doc-excerpts rot fixtures register datagen assertions $([ -f crates/phasekit-xtask/src/gates/features.rs ] && echo features)" \
  bash $SP/gates.sh $w > $SP/gates-$step.out 2>&1
st=$?
grep -v "^ok" $SP/gates-$step.out ; echo "$step gates exit $st ($(grep -c '^ok' $SP/gates-$step.out) ok)"
footer=""; [[ $step =~ ^m[0-9] ]] && footer="Plan-Step: ${step^^}

"
if [ $st -eq 0 ] && [ -n "$(git status --short ci/test-counts.txt)" ]; then
  git add ci/test-counts.txt && git commit -q -m "chore: test counts

${footer}Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>" && echo "$step: counts committed $(git log --format=%h -1)"
fi
