#!/usr/bin/env bash
# move-only-decomposition walkthrough: performs the documented extraction on a
# fixture module and proves the move with a whitespace-collapsed token
# comparison. Fixtures live only under the sandbox root.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"

cat > "$root/sample.rs" <<'EOF'
//! Fixture module used by the walkthrough.

pub fn double(x: i32) -> i32 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles() {
        assert_eq!(double(2), 4);
    }
}
EOF

# Record the inline body before the move, so fidelity is measured, not assumed.
python3 - "$root" <<'PY'
import pathlib, re, sys

root = pathlib.Path(sys.argv[1])
text = (root / "sample.rs").read_text()
body = re.search(r"mod tests \{\n(.*)\n\}\n$", text, re.S).group(1)
(root / "move-old-body.txt").write_text(body + "\n")
PY

# Steps 2-3: extract the inline module into a path-attributed sibling and
# replace it with the declaration.
python3 - "$root" <<'PY'
import pathlib, re, sys

root = pathlib.Path(sys.argv[1])
src = root / "sample.rs"
text = src.read_text()
match = re.search(r"#\[cfg\(test\)\]\nmod tests \{\n(.*)\n\}\n$", text, re.S)
(root / "sample_tests.rs").write_text(match.group(1) + "\n")
src.write_text(text[: match.start()] + '#[cfg(test)]\n#[path = "sample_tests.rs"]\nmod tests;\n')
PY

# Step 4: token streams with all whitespace collapsed decide the fidelity claim.
python3 - "$root" <<'PY'
import pathlib, sys

root = pathlib.Path(sys.argv[1])
def squeeze(text):
    return " ".join(text.split())

old = (root / "move-old-body.txt").read_text()
new = (root / "sample_tests.rs").read_text()
preserved = squeeze(old) == squeeze(new)
(root / "move-fidelity.txt").write_text(f"tokens preserved={'true' if preserved else 'false'}\n")
(root / "move-declaration.txt").write_text(
    "\n".join(line for line in (root / "sample.rs").read_text().splitlines() if line.startswith("mod") or line.startswith("#[path"))
    + "\n"
)
# Out-of-scope control: the fixture file is well under any threshold, so no
# decomposition was required and nothing was silenced to get here.
(root / "decomposition_negative.txt").write_text(
    "out-of-scope: no threshold warning in this fixture beyond the demonstration\n"
)
PY
