#!/usr/bin/env bash
# land.sh <pr>: wait for the PR's checks; squash-merge when all 9 required checks pass. A failed check stops it; a
# cancelled one (GitHub runner-assignment timeouts) is re-run, at most 5 times. Run it in the background; stop it with
# the harness's task stop, not `pkill -f` (the pattern matches the calling shell too).
set -u
ROOT=${PHASEKIT_ROOT:-$HOME/Projects}
pr=$1
cd $ROOT/phasekit || exit 2
sleep 25
reruns=0
while true; do
  s=$(gh pr checks "$pr" --json name,bucket,link 2>/dev/null || echo '[]')
  n=$(jq length <<<"$s"); p=$(jq '[.[]|select(.bucket=="pending")]|length' <<<"$s")
  f=$(jq -r '[.[]|select(.bucket=="fail")]|map(.name)|join(",")' <<<"$s")
  c=$(jq -r '[.[]|select(.bucket=="cancel")]|map(.name)|join(",")' <<<"$s")
  if [ -n "$f" ]; then echo "PR $pr FAILED: $f"; exit 1; fi
  if [ -n "$c" ] && [ "$p" = 0 ]; then
    if [ "$reruns" -ge 5 ]; then echo "PR $pr CANCELLED 5 times: $c"; exit 1; fi
    run=$(jq -r '[.[]|select(.bucket=="cancel")][0].link' <<<"$s" | grep -oE 'runs/[0-9]+' | cut -d/ -f2)
    gh run rerun "$run" --failed >/dev/null 2>&1 && reruns=$((reruns + 1)) && echo "PR $pr: re-ran cancelled $c (run $run)"
    sleep 30; continue
  fi
  if [ "$n" -ge 9 ] && [ "$p" = 0 ]; then break; fi
  sleep 30
done
if gh pr merge "$pr" --squash --delete-branch >/dev/null 2>&1 || [ "$(gh pr view "$pr" --json state --jq .state)" = MERGED ]; then
  echo "PR $pr MERGED as $(gh pr view "$pr" --json mergeCommit --jq '.mergeCommit.oid[0:7]')"
else
  echo "PR $pr checks green but merge failed: $(gh pr view "$pr" --json mergeStateStatus --jq .mergeStateStatus)"; exit 1
fi
