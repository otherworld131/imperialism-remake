# 28 — Documentation

## Overview

Documentation serves three audiences: players (how to play), modders (how to customize),
and developers (how to contribute and understand the codebase).

## Checklist

### Player Documentation
- [ ] **Tutorial**: Interactive in-game tutorial (step-by-step guided first game)
  - [ ] Covers: nation selection, capital placement, civilian deployment, railroad building
  - [ ] Covers: trade setup, diplomacy basics, military basics
  - [ ] Progressive disclosure: introduce systems one at a time
- [ ] **In-game help**: Context-sensitive tooltips on every UI element
- [ ] **Game manual**: comprehensive reference (HTML or PDF)
  - [ ] Overview and victory conditions
  - [ ] Map and terrain guide
  - [ ] Economy and production chains
  - [ ] Technology tree reference (visual diagram + text)
  - [ ] Diplomacy guide
  - [ ] Military unit reference (stats table)
  - [ ] Ship reference (stats table)
  - [ ] Building reference
  - [ ] Transport guide
  - [ ] Hotkey reference card
- [ ] **Strategy guide**: tips for beginners (adapted from wiki strategy guides)

### Modder Documentation
- [ ] **Data file reference**: schema documentation for every definition file
  - [ ] Field descriptions, types, valid ranges, examples
  - [ ] Cross-reference: which fields affect which game mechanics
- [ ] **Mod creation guide**: step-by-step tutorial
  - [ ] Create mod folder, write manifest, override data files
  - [ ] Add new units, techs, nations
  - [ ] Create custom scenarios
- [ ] **Map editor guide**: how to use the map/scenario editor
- [ ] **Localization guide**: how to add a new language translation

### Developer Documentation
- [ ] **Architecture overview**: layer diagram, dependency rules, key abstractions
- [ ] **ADR index**: all Architectural Decision Records with status
- [ ] **Domain model**: entity relationship diagram, aggregate boundaries
- [ ] **Getting started**: clone, build, run, test — in under 5 minutes
- [ ] **Contributing guide**: code style, PR process, test requirements, commit conventions
- [ ] **API documentation**: auto-generated from Rust doc comments (`///`) via `cargo doc` / `rustdoc`
- [ ] **Turn resolution pipeline**: detailed sequence diagram
- [ ] **AI architecture**: decision tree documentation, tuning parameters

### Documentation Maintenance
- [ ] Docs co-located with code where possible (code comments, Rust doc comments (`///`))
- [ ] Reference docs generated from data files (unit stats, tech tree auto-published)
- [ ] Version-stamped: docs match the game version they describe
- [ ] Review: major feature PRs require doc updates

### Verification Strategy
- [ ] **Link check**: All internal doc links are valid (no broken references)
- [ ] **Coverage check**: Every public API has Rust doc comments (`///`)
- [ ] **Data-doc sync**: Auto-generated docs from data files match current definitions
- [ ] **Tutorial test**: Follow tutorial steps on a fresh game → all steps completable, no dead ends
- [ ] **Build docs**: Documentation builds without errors (`cargo doc` or equivalent)
- [ ] **Mod guide test**: Follow mod creation guide → mod loads in game correctly
