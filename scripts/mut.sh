#!/usr/bin/env bash
# mut.sh <worktree> <base> <name>: cargo-mutants on the worktree's Rust diff since <base>, with main's settings
# (nextest, stop at the first failure, the optimised mutants profile, incremental, a 600 s floor). One run at a time:
# two at once oversubscribe the cores and fake TIMEOUTs. Output: $SP/mut/<name>/, summary $SP/mut/<name>.log.
SP=$(dirname "$(readlink -f "$0")"); ROOT=${PHASEKIT_ROOT:-$HOME/Projects}
w=$1; base=$2; name=$3
unset CARGO_TARGET_DIR
export TMPDIR=$HOME/.cache/phasekit-tmp PHASEKIT_REFERENCE_DIR=$ROOT/phasekit/reference CARGO_INCREMENTAL=1
export CARGO_PROFILE_MUTANTS_INHERITS=test CARGO_PROFILE_MUTANTS_OPT_LEVEL=1 CARGO_PROFILE_MUTANTS_DEBUG=false
mkdir -p $TMPDIR $SP/mut; cd $w || exit 2
git diff $base HEAD -- '*.rs' > $SP/mut/$name.diff
rm -rf $SP/mut/$name; s=$(date +%s)
cargo mutants --jobs 3 --test-tool nextest --profile mutants --minimum-test-timeout 600 --cargo-test-arg=--max-fail=1:immediate \
  --in-diff $SP/mut/$name.diff --output $SP/mut/$name > $SP/mut/$name.log 2>&1
echo "$name: $(grep 'mutants tested' $SP/mut/$name.log) [wall $(( $(date +%s) - s )) s]"
for f in missed timeout; do [ -s $SP/mut/$name/mutants.out/$f.txt ] && sed "s/^/  $f: /" $SP/mut/$name/mutants.out/$f.txt; done
exit 0
