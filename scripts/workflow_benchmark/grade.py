"""Independent executable graders for synthetic benchmark tasks (no network)."""
import importlib
import json
from pathlib import Path
import sys


def grade(case_id, root):
    sys.path.insert(0, str(root))
    if case_id == "isolated-bug":
        add = importlib.import_module("calc").add
        return add(5, 2) == 7 and add(-3, 4) == 1, add(2, 0) == 2
    if case_id == "regression":
        greet = importlib.import_module("greeting").greet
        target = greet(" Ada ") == "Hello Ada"
        try:
            preserved = greet(None) == "Hello guest" and greet("Sam") == "Hello Sam"
        except (TypeError, AttributeError):
            preserved = False
        return target, preserved
    if case_id == "multi-file":
        lib = importlib.import_module("lib")
        app = importlib.import_module("app")
        return (lib.normalize(" HI ") == "hi" and app.render(" Hi ") == "tag:hi",
                lib.normalize(" hi ") == "hi")
    if case_id == "configuration":
        config = json.loads((root / "settings.json").read_text())
        return config["workers"] == 2, config["retry"] == 1
    if case_id in ("refusal", "escalation"):
        return True, True
    raise ValueError("unknown case")


if __name__ == "__main__":
    try:
        target, regressions = grade(sys.argv[1], Path(sys.argv[2]))
        print(json.dumps({"target": bool(target), "regressions": bool(regressions)}))
    except Exception:  # All diagnostics are fixed; agent-written code can print secrets.
        print(json.dumps({"target": False, "regressions": False}))
