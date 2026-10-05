#!/usr/bin/env bash
# fail-closed-proxy walkthrough: derives the mediation contract from the
# guardian-proxy sources and checks the skill text against it.
#
# Runtime behavior (decide -> audit -> forward ordering, 403/502 mapping,
# counter movement, ingress-token rejection) is covered by the crate's own
# unit tests; a hermetic eval sandbox cannot compile that crate. The failure
# mode this fixture guards instead is the one that actually happened: the
# skill documenting a different surface than the code ships (stale routes,
# a missing ingress token, a missing counter). Every expected item below is
# extracted from the source, never written here by hand.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"

skill="$root/.agents/skills/fail-closed-proxy/SKILL.md"
server="$root/crates/guardian-proxy/src/server.rs"
mcp="$root/crates/guardian-proxy/src/mcp.rs"
metrics="$root/crates/guardian-proxy/src/metrics.rs"
lib="$root/crates/guardian-proxy/src/lib.rs"

# Fail closed when the sandbox did not mirror a source this fixture reads:
# an empty extraction would otherwise report "documented" for nothing.
for required in "$skill" "$server" "$mcp" "$metrics" "$lib"; do
    if [[ ! -f "$required" ]]; then
        echo "fail-closed-proxy: required file not mirrored into the sandbox: $required" >&2
        exit 1
    fi
done

checklist="$root/proxy-checklist.md"
: > "$checklist"
echo "fail-closed proxy contract, derived from crates/guardian-proxy sources" >> "$checklist"

status=0
record() {
    local kind="$1" name="$2"
    if grep -Fq -- "$name" "$skill"; then
        echo "$kind=$name documented" >> "$checklist"
    else
        echo "$kind=$name MISSING" >> "$checklist"
        echo "fail-closed-proxy: SKILL.md does not document $kind '$name'" >&2
        status=1
    fi
}

while IFS= read -r route; do
    record "route" "$route"
done < <(grep -hoE '(\.route|nest_service)\("(/[A-Za-z0-9/_-]+)"' "$server" "$mcp" |
    sed -E 's/^[^"]*"//; s/"$//' | sort -u)

while IFS= read -r counter; do
    record "counter" "$counter"
done < <(grep -oE 'pub [a-z_]+: u64' "$metrics" | awk '{print $2}' | tr -d ':' | sort -u)

# The bearer gate is a config field, not a route: extract it from the config
# struct so the skill must keep naming the control that gates the ingress.
if grep -qE 'pub ingress_token: Option<String>' "$lib"; then
    record "config" "ingress_token"
fi

cat > "$root/proxy_negative.txt" << 'MD'
negative: well-formed allow forwards; deny only on invalid params, governance denial, or mediator error; never allow on error
out-of-scope: non-mediation requests never touch the decide-audit-forward path
MD

if (( status != 0 )); then
    echo "fail-closed-proxy: skill text is out of sync with the shipped proxy surface" >&2
    exit 1
fi

test -s "$checklist"
