#!/bin/sh
# hook-recorder.sh -- Phase 0 Git behavior probe.
#
# Installed (copied) as post-commit / post-rewrite / post-merge during the Git
# behavior experiments. For every invocation it appends ONE JSON line to
# $DEVCHIEVEMENTS_TRACE_FILE.
#
# Design constraints that come from the product spec (section 9):
#   - must never block Git: no long work, hard timeout budget in the product is
#     200 ms, and here we do a handful of local git calls only;
#   - must never fail the Git operation: every command is guarded, and the last
#     statement is 'exit 0';
#   - must capture the facts the product actually needs, at the moment the hook
#     runs: HEAD sha, parents, author/committer identity+time, branch, the HEAD
#     reflog tail, and the hook's stdin (for post-rewrite).
#
# This is a *probe*, not the product Helper (which will be a Rust binary).

set -u

trace="${DEVCHIEVEMENTS_TRACE_FILE:-}"
if [ -z "$trace" ]; then
  exit 0
fi

hook_name="$1"
shift

# Monotonic-ish timestamp from the shell; the product measures real elapsed time
# from the outer harness, this is only for ordering.
now_ns=$(date +%s%N 2>/dev/null || echo 0)

# post-rewrite passes the rewrite kind as $1 and the mapping on stdin.
rewrite_kind=""
stdin_payload=""
case "$hook_name" in
  post-rewrite)
    rewrite_kind="${1:-}"
    stdin_payload=$(cat 2>/dev/null | tr '\n' '|')
    ;;
esac

head_sha=$(git rev-parse HEAD 2>/dev/null)
head_short=$(git rev-parse --short HEAD 2>/dev/null)
parents=$(git rev-list --parents -n 1 HEAD 2>/dev/null)
branch=$(git symbolic-ref --quiet --short HEAD 2>/dev/null)
author_name=$(git log -1 --pretty=%an HEAD 2>/dev/null)
author_email=$(git log -1 --pretty=%ae HEAD 2>/dev/null)
author_ts=$(git log -1 --pretty=%at HEAD 2>/dev/null)
committer_name=$(git log -1 --pretty=%cn HEAD 2>/dev/null)
committer_email=$(git log -1 --pretty=%ce HEAD 2>/dev/null)
committer_ts=$(git log -1 --pretty=%ct HEAD 2>/dev/null)
subject=$(git log -1 --pretty=%s HEAD 2>/dev/null)
tree=$(git rev-parse HEAD^{tree} 2>/dev/null)

# HEAD reflog tail: the product must not rely on a fixed reflog string, so we
# record exactly what this Git version writes.
reflog_line=$(git reflog -1 --format='%H|%gd|%gs|%gD' 2>/dev/null)
reflog_gs=$(git reflog -1 --format='%gs' 2>/dev/null)
reflog_count=$(git reflog --format='%H' 2>/dev/null | wc -l | tr -d ' ')

# Did HEAD's reflog already contain this commit when the hook ran?
reflog_has_head=0
if [ -n "$head_sha" ]; then
  if git reflog --format='%H' 2>/dev/null | grep -q "^$head_sha$"; then
    reflog_has_head=1
  fi
fi

# Number of commits reachable from HEAD (used to detect squashes/rewrites).
total_commits=$(git rev-list --count HEAD 2>/dev/null)

esc() {
  # Minimal JSON string escaping without needing jq/python.
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/\t/\\t/g'
}

{
  printf '{"hook":"%s"' "$(esc "$hook_name")"
  printf ',"ts_ns":%s' "${now_ns:-0}"
  printf ',"rewrite_kind":"%s"' "$(esc "$rewrite_kind")"
  printf ',"stdin":"%s"' "$(esc "$stdin_payload")"
  printf ',"head_sha":"%s"' "$(esc "$head_sha")"
  printf ',"head_short":"%s"' "$(esc "$head_short")"
  printf ',"parents":"%s"' "$(esc "$parents")"
  printf ',"tree":"%s"' "$(esc "$tree")"
  printf ',"branch":"%s"' "$(esc "$branch")"
  printf ',"author_name":"%s"' "$(esc "$author_name")"
  printf ',"author_email":"%s"' "$(esc "$author_email")"
  printf ',"author_ts":%s' "${author_ts:-0}"
  printf ',"committer_name":"%s"' "$(esc "$committer_name")"
  printf ',"committer_email":"%s"' "$(esc "$committer_email")"
  printf ',"committer_ts":%s' "${committer_ts:-0}"
  printf ',"subject":"%s"' "$(esc "$subject")"
  printf ',"reflog_line":"%s"' "$(esc "$reflog_line")"
  printf ',"reflog_gs":"%s"' "$(esc "$reflog_gs")"
  printf ',"reflog_count":%s' "${reflog_count:-0}"
  printf ',"reflog_has_head":%s' "${reflog_has_head:-0}"
  printf ',"total_commits":%s' "${total_commits:-0}"
  printf ',"cwd":"%s"' "$(esc "$(pwd)")"
  printf ',"git_dir":"%s"' "$(esc "$(git rev-parse --absolute-git-dir 2>/dev/null)")"
  printf ',"common_dir":"%s"' "$(esc "$(git rev-parse --git-common-dir 2>/dev/null)")"
  printf ',"is_worktree":"%s"' "$(esc "$(git rev-parse --is-inside-work-tree 2>/dev/null)")"
  printf ',"git_version":"%s"' "$(esc "$(git --version)")"
  printf '}\n'
} >> "$trace" 2>/dev/null

# Never fail the Git operation.
exit 0
