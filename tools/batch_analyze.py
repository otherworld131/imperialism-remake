#!/usr/bin/env python3
"""Batch-game analyzer for the Imperialism remake.

Reads a batch JSON report (`cargo run --release --bin imperialism -- --batch N`)
and prints a focused, human-readable summary of trade, warehouse, materials,
or any custom field at one or more snapshot turns. Use this to investigate
AI behavior — "is the AI importing coal?", "is the paper chain producing?",
"who's bankrupt?".

The tool is **data-driven**: column lists for warehouse/materials are read
from the snapshot itself, so any new field added on the Rust side shows up
without code changes. Pass `--keys K1,K2` to restrict, or `--list-keys` to
inspect what's available.

Usage examples:
    # Auto-pick year (turn-50 + last) and dump every section:
    cargo run --release --bin imperialism -- --batch 3 > /tmp/batch.json
    python3 tools/batch_analyze.py /tmp/batch.json

    # Specific snapshot year(s):
    python3 tools/batch_analyze.py /tmp/batch.json --year 1830 --year 1875

    # One section only:
    python3 tools/batch_analyze.py /tmp/batch.json --section warehouse

    # Restrict columns:
    python3 tools/batch_analyze.py /tmp/batch.json --section warehouse \\
        --keys Coal,Iron,Timber

    # See what's in a snapshot:
    python3 tools/batch_analyze.py /tmp/batch.json --list-keys

    # Read from stdin:
    cargo run --release --bin imperialism -- --batch 3 \\
        | python3 tools/batch_analyze.py -

Cargo writes a few build-progress lines before the JSON; this tool skips
everything before the first `{` automatically.
"""

import argparse
import json
import sys
from typing import Any


# ── JSON loading ────────────────────────────────────────────────────────────


def load_report(path: str) -> dict:
    """Load a batch JSON report, tolerant of cargo build-progress prelude."""
    if path == "-":
        raw = sys.stdin.read()
    else:
        with open(path) as f:
            raw = f.read()
    start = raw.find("{")
    if start < 0:
        sys.exit(f"error: no JSON object found in {path}")
    return json.loads(raw[start:])


def available_years(report: dict) -> list[str]:
    years: set[str] = set()
    for g in report.get("games", []):
        years.update(g.get("snapshots", {}).keys())
    return sorted(years, key=lambda y: int(y) if y.isdigit() else y)


def select_years(report: dict, requested: list[str]) -> list[str]:
    avail = available_years(report)
    if not requested:
        picks: list[str] = []
        if "1830" in avail:
            picks.append("1830")
        elif avail:
            picks.append(avail[len(avail) // 2])
        if avail and avail[-1] not in picks:
            picks.append(avail[-1])
        return picks
    missing = [y for y in requested if y not in avail]
    if missing:
        sys.exit(
            f"error: requested years not in report: {missing} "
            f"(available: {avail})"
        )
    return requested


def first_snapshot(report: dict) -> dict[str, Any] | None:
    """Return the first non-empty per-nation snapshot dict, for key discovery."""
    for g in report.get("games", []):
        for _year, tn in g.get("snapshots", {}).items():
            if isinstance(tn, dict):
                for _name, n in tn.items():
                    if isinstance(n, dict) and n:
                        return n
    return None


def discover_map_keys(report: dict, field: str) -> list[str]:
    """Union of keys observed in `snapshot[<field>]` across all snapshots."""
    keys: set[str] = set()
    for g in report.get("games", []):
        for tn in g.get("snapshots", {}).values():
            if not isinstance(tn, dict):
                continue
            for n in tn.values():
                if not isinstance(n, dict):
                    continue
                v = n.get(field)
                if isinstance(v, dict):
                    keys.update(v.keys())
    return sorted(keys)


def parse_keys(s: str | None) -> list[str] | None:
    if not s:
        return None
    return [k.strip() for k in s.split(",") if k.strip()]


# ── Formatting helpers ──────────────────────────────────────────────────────


def fmt_money(n: int | float) -> str:
    sign = "-" if n < 0 else ""
    n = abs(n)
    if n >= 1_000_000:
        return f"{sign}${n / 1_000_000:.2f}M"
    if n >= 10_000:
        return f"{sign}${n / 1_000:.1f}k"
    return f"{sign}${int(n)}"


def split_recent_trades(rt: dict[str, int]) -> tuple[list[tuple[str, int]], list[tuple[str, int]]]:
    bought: list[tuple[str, int]] = []
    sold: list[tuple[str, int]] = []
    for k, v in rt.items():
        if ":" not in k:
            continue
        direction, commodity = k.split(":", 1)
        (bought if direction == "bought" else sold).append((commodity, v))
    bought.sort(key=lambda x: -x[1])
    sold.sort(key=lambda x: -x[1])
    return bought, sold


def truncate(s: str, n: int) -> str:
    return s if len(s) <= n else s[: n - 1] + "…"


# ── Sections ────────────────────────────────────────────────────────────────


def section_summary(report: dict, years: list[str], _keys: list[str] | None) -> None:
    print("\n=== SUMMARY ===")
    for gi, g in enumerate(report["games"]):
        winner = g.get("winner") or "—"
        seed = (g.get("seed") or "?")[:8]
        print(f"\nGame {gi} (seed={seed}…) winner={winner}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            n_nations = len(tn)
            bankrupt = sum(1 for n in tn.values() if n.get("treasury", 0) < 0)
            arms_zero = sum(1 for n in tn.values() if n.get("arms", 0) == 0)
            tot_treasury = sum(n.get("treasury", 0) for n in tn.values())
            tot_sales = sum(
                n.get("cash_income_totals", {}).get("AiGoodsSale", 0)
                for n in tn.values()
            )
            tot_purch = sum(
                n.get("cash_expense_totals", {}).get("TradePurchase", 0)
                for n in tn.values()
            )
            print(
                f"  @{y}: nations={n_nations:>2} bankrupt={bankrupt} arms=0={arms_zero} "
                f"sum_treasury={fmt_money(tot_treasury)} "
                f"sum_sales={fmt_money(tot_sales)} sum_imports={fmt_money(tot_purch)}"
            )


def section_trade(report: dict, years: list[str], keys: list[str] | None) -> None:
    """Bought/sold quantities over the trailing 4 turns of each snapshot.

    Optional `keys` restrict which commodities are shown (e.g. 'Coal,Iron').
    """
    print("\n=== TRADE (last 4 turns of each snapshot) ===")
    if keys:
        print(f"  filter: {keys}")
    for gi, g in enumerate(report["games"]):
        print(f"\nGame {gi} winner={g.get('winner') or '—'}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            print(f"  --- @{y} ---")
            for name, n in sorted(tn.items()):
                rt = n.get("recent_trades", {})
                if not rt:
                    continue
                bought, sold = split_recent_trades(rt)
                if keys:
                    bought = [(c, q) for c, q in bought if c in keys]
                    sold = [(c, q) for c, q in sold if c in keys]
                b = " ".join(f"{c}={q}" for c, q in bought[:8]) if bought else "—"
                s = " ".join(f"{c}={q}" for c, q in sold[:8]) if sold else "—"
                print(f"    {name:<16} BOUGHT: {b}")
                print(f"    {' ' * 16} SOLD  : {s}")


def section_table(report: dict, years: list[str], keys: list[str] | None,
                  field: str, title: str) -> None:
    """Tabular view of `snapshot[<field>]` (a map of name→u32) per nation."""
    cols = keys or discover_map_keys(report, field)
    if not cols:
        print(f"\n=== {title} ===\n  (no '{field}' data in report)")
        return
    print(f"\n=== {title} ===")
    col_w = 7
    header_cols = " ".join(f"{truncate(c, col_w):>{col_w}}" for c in cols)
    header = "  " + f"{'Nation':<16} " + header_cols
    for gi, g in enumerate(report["games"]):
        print(f"\nGame {gi} winner={g.get('winner') or '—'}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            print(f"  --- @{y} ---")
            print(header)
            for name, n in sorted(tn.items()):
                d = n.get(field, {}) or {}
                row = " ".join(f"{d.get(c, 0):>{col_w}}" for c in cols)
                print(f"  {name:<16} {row}")


def section_warehouse(report: dict, years: list[str], keys: list[str] | None) -> None:
    section_table(report, years, keys, "warehouse", "WAREHOUSE (raw resources)")


def section_materials(report: dict, years: list[str], keys: list[str] | None) -> None:
    section_table(report, years, keys, "materials", "MATERIALS (manufactured)")


def section_paper(report: dict, years: list[str], _keys: list[str] | None) -> None:
    """Verify paper chain: stockpile + sells / buys.

    Paper has no dedicated cumulative-production counter, but production is
    inferred from `materials.Paper > 0` OR `recent_trades.sold:Paper > 0`.
    Pure imports (`bought:Paper > 0` with stock=0 and no sells) signal a
    broken chain.
    """
    print("\n=== PAPER CHAIN ===")
    for gi, g in enumerate(report["games"]):
        print(f"\nGame {gi} winner={g.get('winner') or '—'}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            print(f"  --- @{y} ---")
            producing = 0
            silent = 0
            for name, n in sorted(tn.items()):
                stock = n.get("materials", {}).get("Paper", 0)
                rt = n.get("recent_trades", {})
                bought_paper = rt.get("bought:Paper", 0)
                sold_paper = rt.get("sold:Paper", 0)
                workers = n.get("worker_count", 0)
                makes = stock > 0 or sold_paper > 0
                tag = "OK  " if makes else "----"
                if makes:
                    producing += 1
                else:
                    silent += 1
                print(
                    f"    {name:<16} {tag} stock={stock:>3} "
                    f"sold4t={sold_paper:>3} bought4t={bought_paper:>3} workers={workers}"
                )
            print(f"    -> producing={producing}/{producing + silent} silent={silent}")


def section_bankruptcy(report: dict, years: list[str], _keys: list[str] | None) -> None:
    print("\n=== BANKRUPTCY ===")
    for gi, g in enumerate(report["games"]):
        print(f"\nGame {gi} winner={g.get('winner') or '—'}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            broke = [
                (name, n["treasury"])
                for name, n in tn.items()
                if n.get("treasury", 0) < 0
            ]
            if not broke:
                print(f"  @{y}: no bankrupt nations")
            else:
                print(f"  @{y}: {len(broke)} bankrupt:")
                for name, t in sorted(broke, key=lambda x: x[1]):
                    print(f"    {name:<16} treasury={fmt_money(t)}")


def section_field(report: dict, years: list[str], keys: list[str] | None,
                  field: str | None) -> None:
    """Print one scalar snapshot field (e.g. `army_size`, `tech_count`).

    `field` is supplied via `--field`. Use `--list-keys` to discover.
    """
    if not field:
        sys.exit("error: --section field requires --field <name>")
    print(f"\n=== FIELD '{field}' ===")
    for gi, g in enumerate(report["games"]):
        print(f"\nGame {gi} winner={g.get('winner') or '—'}")
        for y in years:
            tn = g["snapshots"].get(y, {})
            print(f"  --- @{y} ---")
            for name, n in sorted(tn.items()):
                v = n.get(field, "—")
                print(f"    {name:<16} {field}={v}")


# ── Section dispatch ────────────────────────────────────────────────────────


SECTIONS = {
    "summary": section_summary,
    "trade": section_trade,
    "warehouse": section_warehouse,
    "materials": section_materials,
    "paper": section_paper,
    "bankruptcy": section_bankruptcy,
}


def list_keys(report: dict) -> None:
    snap = first_snapshot(report)
    if not snap:
        sys.exit("error: no snapshots in report")
    print("Snapshot scalar fields:")
    for k, v in sorted(snap.items()):
        kind = type(v).__name__
        print(f"  {k:<28} <{kind}>")
    print("\nMap fields (use --keys to filter):")
    for k, v in sorted(snap.items()):
        if isinstance(v, dict):
            sample = ", ".join(list(v.keys())[:6])
            print(f"  {k:<28} keys → {sample}{'…' if len(v) > 6 else ''}")


def main() -> int:
    p = argparse.ArgumentParser(
        description="Analyze an Imperialism batch JSON report.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    p.add_argument(
        "path",
        nargs="?",
        default="-",
        help="Path to batch JSON (or '-'/omit for stdin). Cargo build-progress "
        "lines before the first '{' are tolerated.",
    )
    p.add_argument(
        "--year",
        action="append",
        default=[],
        help="Snapshot year to inspect (e.g. 1830). Repeat for multiple. "
        "Default: 1830 + last available.",
    )
    p.add_argument(
        "--section",
        choices=list(SECTIONS) + ["field", "all"],
        default="all",
        help="Section to print, or 'all' (default), or 'field' (with --field).",
    )
    p.add_argument(
        "--keys",
        help="Comma-separated keys to restrict map sections (warehouse/"
        "materials/trade). Default: all keys discovered in the report.",
    )
    p.add_argument(
        "--field",
        help="Scalar field name (used with --section field, e.g. 'army_size').",
    )
    p.add_argument(
        "--list-keys",
        action="store_true",
        help="Print every snapshot key (scalar fields + map fields) and exit.",
    )
    args = p.parse_args()

    report = load_report(args.path)

    if args.list_keys:
        list_keys(report)
        return 0

    years = select_years(report, args.year)
    keys = parse_keys(args.keys)

    if args.section == "field":
        section_field(report, years, keys, args.field)
        return 0
    sections = list(SECTIONS) if args.section == "all" else [args.section]
    for s in sections:
        SECTIONS[s](report, years, keys)
    return 0


if __name__ == "__main__":
    sys.exit(main())
