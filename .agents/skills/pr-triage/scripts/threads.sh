#!/usr/bin/env bash
# Review-thread and issue-comment helpers for a PR.
#
# Usage:
#   threads.sh list <pr>            review threads (TSV: id resolved outdated path line author excerpt)
#   threads.sh resolve <thread-id>  resolve one review thread
#   threads.sh issue-comments <pr>  top-level comments (TSV: author created_at excerpt)
set -euo pipefail

cmd="${1:?usage: threads.sh <list|resolve|issue-comments> ...}"

resolve_query=$(cat <<'GQL'
mutation ResolveThread($threadId: ID!) {
  resolveReviewThread(input: {threadId: $threadId}) { thread { id isResolved } }
}
GQL
)

list_query=$(cat <<'GQL'
query Threads($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          comments(first: 1) { nodes { author { login } body } }
        }
      }
    }
  }
}
GQL
)

case "$cmd" in
  resolve)
    thread_id="${2:?usage: threads.sh resolve <thread-id>}"
    gh api graphql -f query="$resolve_query" -f threadId="$thread_id" \
      --jq '.data.resolveReviewThread.thread | "resolved=\(.isResolved) id=\(.id)"'
    ;;

  list)
    pr="${2:?usage: threads.sh list <pr-number>}"
    owner=$(gh repo view --json owner --jq .owner.login)
    name=$(gh repo view --json name --jq .name)
    cursor=""
    while :; do
      args=(-f query="$list_query" -f owner="$owner" -f name="$name" -F number="$pr")
      if [ -n "$cursor" ]; then
        args+=(-f after="$cursor")
      fi
      out=$(gh api graphql "${args[@]}" \
        --jq '.data.repository.pullRequest.reviewThreads | (.pageInfo.hasNextPage|tostring), (.pageInfo.endCursor // ""), (.nodes[] | [.id, (.isResolved|tostring), (.isOutdated|tostring), (.path // ""), (.line // 0 | tostring), (.comments.nodes[0].author.login // ""), ((.comments.nodes[0].body // "") | gsub("\n"; " ") | .[0:80])] | @tsv)')
      has_next=$(printf '%s\n' "$out" | sed -n '1p')
      cursor=$(printf '%s\n' "$out" | sed -n '2p')
      printf '%s\n' "$out" | sed -n '3,$p'
      [ "$has_next" = "true" ] || break
    done
    ;;

  issue-comments)
    pr="${2:?usage: threads.sh issue-comments <pr-number>}"
    gh api "repos/{owner}/{repo}/issues/$pr/comments?per_page=100" \
      --jq '.[] | [(.user.login // ""), .created_at, ((.body // "") | gsub("\n"; " ") | .[0:120])] | @tsv'
    ;;

  *)
    printf 'usage: threads.sh <list|resolve|issue-comments> ...\n' >&2
    exit 2
    ;;
esac
