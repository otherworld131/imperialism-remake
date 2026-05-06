---
name: batch-analyze
description: Analyze the JSON output of `cargo run --bin imperialism -- --batch N` to investigate AI economy/trade/military behavior. Use when debugging AI decisions, verifying balance changes (e.g. "is the AI importing coal?", "is the paper chain producing?", "any bankruptcies?") or asked to inspect a batch report.
---

# Batch Analyzer

Inspects an Imperialism batch-game JSON report (`tools/batch_analyze.py`) and prints focused, diff-friendly summaries of trade flow, raw-resource warehouse, manufactured-material stockpiles, paper-chain health, bankruptcies, or any custom snapshot field.

The tool is **data-driven** — column lists are read from the snapshot itself, so any new field added to `src/batch.rs::NationSnapshot` shows up automatically.

## When to use

Trigger this skill when the user asks anything like:
- "Are AI nations importing Coal/Iron/Timber?"
- "Why is treasury / arms / steel / [resource] going up/down?"
- "Is the [chain] still producing?"
- "Did anyone go bankrupt by turn N?"
- "Show me the trade activity for [game / nation / year]."
- "What's in the warehouse at turn 50?"

Or anytime the user has just run `--batch N` and wants a readable summary of what happened.

## Quick start

```bash
# 1. Run a batch (or use an existing JSON file).
cargo run --release --bin imperialism -- --batch 3 > /tmp/batch.json

# 2. Default view: summary + every section, at year 1830 + last available.
python3 tools/batch_analyze.py /tmp/batch.json

# 3. One section, narrowed to a few keys:
python3 tools/batch_analyze.py /tmp/batch.json \
    --section warehouse --keys Coal,Iron,Timber

# 4. Read from stdin (skips cargo build-progress prelude automatically):
cargo run --release --bin imperialism -- --batch 3 | python3 tools/batch_analyze.py -

# 5. Discover every field in a snapshot before slicing:
python3 tools/batch_analyze.py /tmp/batch.json --list-keys
```

## CLI surface

```
usage: batch_analyze.py [-h] [--year YEAR] [--section S] [--keys K1,K2]
                        [--field F] [--list-keys]
                        [path]

path           Path to batch JSON, or '-'/omitted for stdin.

--year         Snapshot year to inspect (e.g. 1830). Repeat to inspect
               multiple. Default: 1830 + last available.
--section      summary | trade | warehouse | materials | paper |
               bankruptcy | field | all  (default: all)
--keys         Comma-separated keys to restrict map sections (warehouse,
               materials, trade). Default: every key in the report.
--field        Scalar field name (used with `--section field`).
--list-keys    Print every snapshot key and exit. Use this to discover
               what's available before filtering.
```

## Sections

| Section      | What it answers                                                            |
|--------------|----------------------------------------------------------------------------|
| `summary`    | Per-game treasury / sales / imports totals and bankruptcy/arms-zero counts |
| `trade`      | Bought/sold quantities over the trailing 4 turns of each snapshot          |
| `warehouse`  | Per-nation table of raw resources (Coal, Iron, Timber, …)                  |
| `materials`  | Per-nation table of manufactured materials (Lumber, Steel, Paper, Arms, …) |
| `paper`      | Verifies the paper chain — stock + sells/buys vs worker count              |
| `bankruptcy` | Per-snapshot list of nations with treasury < 0                             |
| `field`      | Any scalar snapshot field (use with `--field name`)                        |

## Recipes

**"Is the AI importing Coal/Iron?"**
```bash
python3 tools/batch_analyze.py /tmp/batch.json --section trade --keys Coal,Iron
```

**"Did the paper chain regress after my change?"**
```bash
python3 tools/batch_analyze.py /tmp/batch.json --section paper
```

**"Who's bankrupt and when?"**
```bash
python3 tools/batch_analyze.py /tmp/batch.json --section bankruptcy \
    --year 1830 --year 1860 --year 1890 --year 1915
```

**"What's everyone's army size at end-game?"**
```bash
python3 tools/batch_analyze.py /tmp/batch.json --section field --field army_size --year 1915
```

## Reading the output

`OK` / `----` flags in the paper section: `OK` if `material_stock > 0` OR `sold:Paper` was non-zero in the trailing window; `----` otherwise (chain idle).

`recent_trades` keys are `bought:<commodity>` or `sold:<commodity>`. The `<commodity>` matches whatever the trade-history entry recorded — typically a `ResourceType` (Coal, Iron, …), `MaterialType` (Lumber, Steel, …) or `GoodsType` (Furniture, Hardware, Clothing) name.

Bankruptcies are an instant red flag — under the current AI logic the buy-side cash floor should prevent them, so any non-zero count after turn 5 is a real bug.

## When to extend the tool

Add a new map field to `src/batch.rs::NationSnapshot` and rebuild. The analyzer's `--list-keys` and `discover_map_keys()` will surface it automatically. Add a new dedicated section (e.g. `military`, `trade-pairs`) by registering a function in the `SECTIONS` dict in `tools/batch_analyze.py`.
