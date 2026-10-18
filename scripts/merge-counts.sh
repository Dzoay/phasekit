#!/bin/sh
# git's merge driver for ci/test-counts.txt (PLAN.md M25.1; user decision LM3), set up as CONTRIBUTING.md "Merging the
# test counts" says: `merge-counts.sh %O %A %B`. `cargo xtask merge-counts` merges the minimums per binary; where that
# does not decide, or xtask does not build mid-rebase, git merges the text, leaving conflict markers.
cargo xtask merge-counts "$1" "$2" "$3" 2>/dev/null && exit 0
exec git merge-file -L ours -L base -L theirs "$2" "$1" "$3"
