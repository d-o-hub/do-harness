#!/usr/bin/env python3
"""
token-usage.py — Record agent token usage as a metrics event.

OpenCode persists per-session token counters in its local SQLite database
(`session_v2`). This script reads that database **read-only**, selects one
session (or several), and writes an AGENTS.md section 6 metrics event under
`.agents/events/YYYY/MM/DD/` so usage can be compared across sessions later.

It never writes to the OpenCode database, and it never reads credentials:
only the token/cost counters and session metadata are selected.

Usage:
  python3 scripts/token-usage.py                      # latest session in this directory
  python3 scripts/token-usage.py --session ses_abc    # one specific session
  python3 scripts/token-usage.py --all --limit 20     # every recent session
  python3 scripts/token-usage.py --report             # print, write nothing
  python3 scripts/token-usage.py --report --format json

Options:
  --db PATH        OpenCode database (default: $XDG_DATA_HOME/opencode/opencode.db)
  --session ID     Restrict to one session id (may be repeated)
  --all            Report every session in this directory, newest first
  --limit N        Cap the number of sessions (default 1, or 20 with --all)
  --out FILE       Event file path (default: the dated events directory)
  --report         Print the report only; write no file
  --format text|json
"""

import argparse
import datetime as dt
import json
import os
import sqlite3
import sys
from pathlib import Path
from urllib.parse import quote

EVENT_KIND = "metrics"
DEFAULT_DATA_HOME = Path.home() / ".local" / "share" / "opencode"


def default_db_path() -> Path:
    data_home = os.environ.get("XDG_DATA_HOME")
    base = Path(data_home) if data_home else DEFAULT_DATA_HOME
    return base / "opencode.db"


def repo_root() -> Path:
    """Nearest ancestor containing `.git` (a worktree's `.git` is a file)."""
    for candidate in [Path.cwd(), *Path.cwd().parents]:
        if (candidate / ".git").exists():
            return candidate
    return Path.cwd()


def connect_readonly(path: Path) -> sqlite3.Connection:
    """Open the database without write capability.

    The URI carries the path percent-encoded, so a directory containing `?` or
    `#` is not silently truncated into a different file. `mode=ro` makes every
    write fail rather than creating a second writer against a live database.
    """
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise SystemExit(f"token-usage: OpenCode database not found: {resolved}")
    uri = f"file:{quote(str(resolved))}?mode=ro"
    try:
        con = sqlite3.connect(uri, uri=True)
    except sqlite3.Error as err:
        raise SystemExit(f"token-usage: cannot open {resolved} read-only: {err}") from err
    con.row_factory = sqlite3.Row
    return con


SESSION_QUERY = (
    "SELECT id, title, directory, model, agent, "
    "tokens_input, tokens_output, tokens_reasoning, "
    "tokens_cache_read, tokens_cache_write, cost, "
    "time_created, time_updated "
    "FROM session_v2 ORDER BY time_updated DESC"
)
# A session table is one row per conversation on this machine — thousands at
# most — so the filters run in Python rather than assembling a WHERE clause.
# Reading the whole ordered table also keeps the statement a constant, which is
# what makes the parameterisation audit trivial.
MAX_SCANNED_ROWS = 100_000


def fetch_sessions(con, ids, directory, every, limit):
    """Select sessions newest-first, then apply the id/directory filters.

    The query carries no caller input: the only variable part is the column
    list, which is a module constant built from a literal. Filtering and
    limiting happen after the read so no string ever reaches the SQL text.
    """
    rows = con.execute(SESSION_QUERY).fetchmany(MAX_SCANNED_ROWS)
    wanted = set(ids) if ids else None
    selected = []
    for row in rows:
        if wanted is not None:
            if row["id"] not in wanted:
                continue
        elif every and directory and row["directory"] != str(directory):
            continue
        selected.append(row)
    if wanted is not None:
        return selected
    return selected[:limit]


def usage_of(row) -> dict:
    """Token counters for one session, with a cache-inclusive total.

    `total` counts input + output only. Cache reads are reported separately
    because they bill at a different rate and would otherwise dominate the
    number without describing new context.
    """
    input_tokens = row["tokens_input"] or 0
    output_tokens = row["tokens_output"] or 0
    return {
        "input": input_tokens,
        "output": output_tokens,
        "reasoning": row["tokens_reasoning"] or 0,
        "cache_read": row["tokens_cache_read"] or 0,
        "cache_write": row["tokens_cache_write"] or 0,
        "total": input_tokens + output_tokens,
        "cost_usd": round(row["cost"] or 0.0, 6),
    }


def iso(ms) -> str | None:
    if not ms:
        return None
    return dt.datetime.fromtimestamp(ms / 1000).astimezone().isoformat(timespec="seconds")


def summarise(rows) -> dict:
    sessions = []
    for row in rows:
        sessions.append(
            {
                "id": row["id"],
                "title": row["title"],
                "directory": row["directory"],
                "model": row["model"],
                "agent": row["agent"],
                "started": iso(row["time_created"]),
                "updated": iso(row["time_updated"]),
                "usage": usage_of(row),
            }
        )
    totals = {
        key: sum(s["usage"][key] for s in sessions)
        for key in ("input", "output", "reasoning", "cache_read", "cache_write", "total")
    }
    totals["cost_usd"] = round(sum(s["usage"]["cost_usd"] for s in sessions), 6)
    return {"sessions": sessions, "totals": totals}


def render_text(report) -> str:
    lines = []
    for s in report["sessions"]:
        u = s["usage"]
        lines.append(f"session {s['id']}  ({s['title'] or 'untitled'})")
        lines.append(f"  window   {s['started']} .. {s['updated']}")
        lines.append(f"  model    {s['model']}  agent {s['agent']}")
        lines.append(
            f"  tokens   in={u['input']} out={u['output']} reasoning={u['reasoning']} "
            f"cache_read={u['cache_read']} cache_write={u['cache_write']}"
        )
        lines.append(f"  total    {u['total']} billable input+output, cost ${u['cost_usd']}")
    t = report["totals"]
    lines.append(
        f"TOTAL       in={t['input']} out={t['output']} reasoning={t['reasoning']} "
        f"cache_read={t['cache_read']} cache_write={t['cache_write']} "
        f"total={t['total']} cost=${t['cost_usd']}"
    )
    return "\n".join(lines)


def event_payload(report) -> dict:
    sessions = report["sessions"]
    first = sessions[0] if len(sessions) == 1 else None
    title = (
        f"Agent token usage: {first['title']}" if first and first["title"] else "Agent token usage"
    )
    return {
        "date": dt.date.today().isoformat(),
        "kind": EVENT_KIND,
        "title": title,
        "source": "scripts/token-usage.py (OpenCode session_v2 counters, read-only)",
        "usage": report,
        "notes": (
            "total is input+output only; reasoning is a subset of output and cache "
            "reads/writes bill separately, so neither is folded into total."
        ),
    }


def default_out(root: Path) -> Path:
    today = dt.date.today()
    stamp = dt.datetime.now().strftime("%H%M%S")
    return (
        root
        / ".agents"
        / "events"
        / f"{today:%Y}"
        / f"{today:%m}"
        / f"{today:%d}"
        / f"token-usage-{stamp}.json"
    )


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description="Record agent token usage as a metrics event"
    )
    parser.add_argument("--db", type=Path, default=None, help="OpenCode database path")
    parser.add_argument(
        "--session", action="append", default=[], metavar="ID", help="session id (repeatable)"
    )
    parser.add_argument("--all", action="store_true", help="every session in this directory")
    parser.add_argument("--limit", type=int, default=None, help="cap sessions reported")
    parser.add_argument("--directory", default=None, help="filter sessions by directory")
    parser.add_argument("--out", type=Path, default=None, help="event file path")
    parser.add_argument("--report", action="store_true", help="print only; write no file")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args(argv)

    db = args.db or default_db_path()
    limit = args.limit if args.limit is not None else (20 if args.all else 1)
    directory = args.directory
    if directory is None and not args.session:
        directory = str(Path.cwd())

    con = connect_readonly(db)
    try:
        rows = fetch_sessions(con, args.session, directory, args.all, limit)
    finally:
        con.close()

    if not rows:
        print(
            f"token-usage: no sessions matched (db={db}, directory={directory}, "
            f"sessions={args.session or 'none'})",
            file=sys.stderr,
        )
        return 1

    report = summarise(rows)

    if args.format == "json":
        print(json.dumps(report, indent=2))
    else:
        print(render_text(report))

    if args.report:
        return 0

    root = repo_root()
    out = args.out or default_out(root)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(event_payload(report), indent=2) + "\n", encoding="utf-8")
    print(f"token-usage: wrote {out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
