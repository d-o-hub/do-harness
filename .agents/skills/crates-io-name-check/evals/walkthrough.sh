#!/usr/bin/env bash
# crates-io-name-check walkthrough: runs the skill's probe shape against a
# local stub of the crates.io API so the graded residue is produced by curl,
# not written by hand.
#
# The stub answers the three status classes the skill classifies:
#   404 => candidate name available, 200 => taken, and 403 => the request was
#   rejected because it carried no User-Agent header.
set -euo pipefail
root="${DO_HARNESS_ROOT:-$(pwd)}"
port_file="$root/crates_io_stub_port.txt"
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT

python3 - "$port_file" <<'PY' &
import http.server
import sys

port_file = sys.argv[1]


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        if not self.headers.get("User-Agent"):
            self.send_response(403)
        elif self.path.endswith("/do-harness-newcrate"):
            self.send_response(404)
        elif self.path.endswith("/do-harness"):
            self.send_response(200)
        else:
            self.send_response(500)
        self.end_headers()

    def log_message(self, fmt, *args):  # noqa: ARG002
        pass


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
with open(port_file, "w", encoding="utf-8") as handle:
    handle.write(str(server.server_address[1]))
server.serve_forever()
PY
server_pid=$!

for _ in $(seq 1 50); do
    [[ -s "$port_file" ]] && break
    sleep 0.1
done
port="$(cat "$port_file")"
base="http://127.0.0.1:${port}/api/v1/crates"
ua="do-harness-name-check (https://github.com/d-o-hub/do-harness)"

output="$root/name_check_output.txt"
: > "$output"
probe() {
    local name="$1"
    local status
    status="$(curl -sS -o /dev/null -w '%{http_code}' \
        -H 'Accept: application/json' -A "$ua" "$base/$name")"
    case "$status" in
        404) echo "status=404 crate=$name available=true" >> "$output" ;;
        200) echo "status=200 crate=$name taken=true" >> "$output" ;;
        *) echo "status=$status crate=$name unexpected=true" >> "$output" ;;
    esac
}

probe do-harness-newcrate
probe do-harness

# The User-Agent requirement is part of the contract: without one the endpoint
# rejects the probe (403), which must surface as a rejection, not "available".
status_no_ua="$(curl -sS -o /dev/null -w '%{http_code}' \
    -H 'Accept: application/json' -H 'User-Agent:' "$base/do-harness-newcrate")"
echo "status=$status_no_ua crate=do-harness-newcrate user_agent=missing" >> "$output"

cat <<'EOF' > "$root/crates_io_negative.txt"
out-of-scope
EOF
