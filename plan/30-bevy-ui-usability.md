# 30 — Bevy UI Usability Improvements

Source: full-screen usability review of the native Bevy GUI (2026-07-11),
one screenshot per screen at the 125% default interface scale, after the
pixel-art map/font/title-screen work landed. Focus: interface usability —
the screens work but read as "messy to use".

This is a **tracking document**: check items off as they land, and keep
the group checklist under "Execution order" in sync. Items are grouped by
priority (P1 = biggest usability wins, P3 = polish). Every item names the
primary files and a verification strategy. Screenshot verification uses
the existing debug hooks (`MAP_SCREENSHOT` + `M6..M10_DEBUG` /
`INTRO_DEBUG` scripts — see `crates/presentation/src/app.rs`).

**Before/after requirement (mandatory):** every implemented change is
presented to the user as a *pair* of screenshots — the same screen, same
debug script, same zoom/scale, captured once from the state before the
change ("before", e.g. via `git stash` or the parent commit) and once
after ("after") — so the improvement is directly comparable. Keep both
until the user has reviewed them.

---

## Cross-cutting principles (apply to every item)

These five findings recur on almost every screen. Individual checklist
items below reference them as **CC-1..CC-5**.

- **CC-1 Containment.** Logical groups (a production chain, a warehouse
  section, an army list) must sit inside a visible container — the widget
  kit's inset panel style (`theme::INSET_BG` + 1px `theme::BORDER`, 4px
  radius, 8–10px padding) — not float as bare text on the screen
  background separated only by gold headings.
- **CC-2 Alarm-color policy.** Red is reserved for "this hurts you next
  turn" (starvation, bankruptcy, unit loss). Amber/gold for "shortfall you
  may want to fix". Neutral gray for routine zero/negative numbers (AI
  treasury deltas, empty allocations on turn 1). Never color a whole row
  red for a state the player hasn't had a chance to act on yet.
- **CC-3 Dead ends carry directions.** Every disabled state ("No
  building", "Not enough arms", "Insufficient resources") gets one
  actionable hint naming the screen/control that unblocks it, e.g.
  "Not enough arms — set the Steel→Arms slider in Industry (F3)".
- **CC-4 No cryptic micro-labels.** Compact indicators (`0/6`, `▼4`,
  `∞`, `35 (35)`, the Trade "GP" dot) get either a real label, an
  inline legend, or at minimum a `widgets::TooltipText`. Meaning-bearing
  ones (shortfall vs. demand) get words, not just symbols.
- **CC-5 Uniform screen chrome.** One pattern for every full-screen
  overlay: title left, screen-specific tabs center, "Close (Esc)"
  top-right, primary action bottom-right. News and Battles currently
  deviate; Diplomacy is a map mode and exempt.

---

## P1 — Industry screen redesign (`screens/industry.rs`)

The messiest screen: three unbounded text columns, nearly every element in
a dead state with no guidance, mixed affordances (some chains have
sliders, some only text).

- [x] **Production-chain cards.** Each chain (Timber→Lumber,
  Lumber→Furniture, Coal+Iron→Steel, Steel→Hardware, Steel→Arms,
  Cotton/Wool→Fabric, Fabric→Clothing, Grain+Fruit+Meat→Canned) becomes a
  card (CC-1): chain icon(s) + arrow diagram, building status line,
  output-allocation slider *when a building exists*. Chains without a
  building render one muted card with the build hint (CC-3), not a
  heading + two gray lines.
  - Verify: `M7_DEBUG=industry` screenshot — every chain visually
    contained; no bare "No building" text outside a card.
- [x] **Name the sliders.** The unlabeled sliders (currently only next to
  Steel→Arms and Canned) get a label ("Output") and the `∞` toggle gets a
  tooltip ("Unlimited — produce as inputs allow") (CC-4).
  - Verify: screenshot + `TooltipText` present in code review.
- [x] **Collapse Army Recruitment while locked.** Nine rows of "Not
  enough arms" collapse into one disabled group: header, muted unit list
  (name + cost only, single line each), and ONE explanation with the
  unblock hint (CC-3). Expands to full rows once `Arms > 0`.
  - Verify: `M7_DEBUG=industry` screenshot on turn 1 (collapsed) and a
    scripted variant after producing arms (expanded) —
    `M7_DEBUG=industryarms` buys arms on the market and resolves two
    turns first.
- [x] **Label the header treasury.** `$10000` → "Treasury $10,000"
  (thousands separator; the Ledger and Tech screens already write it
  this way).
  - Verify: screenshot.
- [x] **Hide AI aims behind debug.** The green "aim N" annotations in the
  Warehouse column are AI-target debug data; show them only when the
  existing "Debug: show AI targets" checkbox is on.
  - Verify: screenshot with toggle off shows no green aims.
- [x] **Warehouse grouping.** Resources / Materials / Goods columns get
  inset containers (CC-1) and right-aligned counts; keep icons.
  - Verify: screenshot.

## P1 — Transport screen (`screens/transport.rs`)

- [x] **Alarm-color pass (CC-2).** Commodity rows are neutral by default;
  amber border when allocation < food requirement; red ONLY when the
  projected result is starvation this turn (workers unfed — projected
  from warehouse stock + deliveries + canned-food fallback, mirroring
  the turn processor's meal logic). On turn 1 with zero allocations,
  nothing should be red.
  - Verify: `M7_DEBUG=transport` screenshot on turn 1 — no red rows.
- [x] **"Auto-fill" button.** One click allocates capacity to meet food
  requirements first (Grain → Fruit → Meat), then remaining capacity by
  warehouse availability. Lives next to the "Transport Allocation"
  header. This is the action every player performs manually every game
  start.
  - Verify: scripted click (`M7_DEBUG=transportfill`) — allocations
    non-zero, food requirement met.
- [x] **Legible capacity labels (CC-4).**
  - [x] "Capacity: 35 (35)" → "35 of 35 cars free".
  - [x] Stepper `0/6` → tooltip "hauling 0 of 6 available at your depots".
  - [x] Red `▼4` → amber "4 short" text (with explanatory tooltip).
  - Verify: screenshot.
- [x] **Bigger stepper targets.** +/− buttons to ≥ 26px square at 100%
  scale; add shift-click = ±5 (document in tooltip).
  - Verify: screenshot + manual click test.
- [x] **Map mode on open.** Opening Transport (F2) switches the map to
  Terrain mode with `show_transport_network` on, so the rail network the
  panel talks about is actually visible; restore the previous mode on
  close.
  - Verify: `M7_DEBUG=transport` screenshot shows terrain + rails behind
    the panel.

## P1 — Cross-cutting passes

- [x] **Red-color audit (CC-2) across all screens.** Centralize the
  choice: add `theme::{ALARM, WARN, MUTED}` constants and replace ad-hoc
  `Color::srgb...` reds in screens.
  - [x] `theme` constants added; screens use them.
  - [x] Ledger: AI treasury deltas → neutral.
  - [x] Industry: "Not enough arms" → muted gray with amber hint.
  - [x] Transport: covered by its alarm-color item (cross-check).
  - [x] News: "FINANCIAL CRISIS" stays red (correct usage — confirmed; crisis category color).
  - Verify: grep — no per-screen hardcoded alarm reds; before/after
    screenshots of Ledger + Industry + Transport.
- [x] **Dead-end hint audit (CC-3).** Sweep every "No X" / "Not enough X"
  / "Insufficient X" string in `screens/` and attach the unblock hint.
  - [x] `industry.rs` (buildings, arms, resources)
  - [x] `transport.rs`
  - [x] `trade.rs` (cargo capacity)
  - [x] `diplomacy.rs` (queued action requirements)
  - [x] `battles.rs` (empty archive)
  - [x] `setup/capital.rs` (suggestion rows carry place + yield detail; invalid-tile hint already present)
  - Verify: grep for the strings; each has an adjacent hint or tooltip.
- [x] **Chrome standardization (CC-5).**
  - [x] News: "Close (Esc)" top-right; Continue stays bottom-right as
    primary; "Back to Map" folds into Close.
  - [x] Battles: align Current/Archive with the tab widget used by
    Trade/Ledger.
  - [x] Esc closes every full-screen overlay (News included via
    `Screen::is_full_screen`; verified).
  - Verify: screenshots of News/Battles/Trade/Ledger headers match.

## P2 — Map screen & side panel (`screens/side_panel.rs`, `map_hud.rs`)

- [x] **Reorder side panel: Nations above Debug.** Nations is gameplay
  information; Debug is developer UI. Order: selected-info, legend,
  player-flow sections, UI section, Nations, Debug.
  - Verify: `HUMAN_GAME=1` screenshot.
- [x] **Collapse Debug by default.** "Debug ▸" disclosure row (persist
  expanded state in `settings.json` alongside `ui_scale`).
  - Verify: screenshot — Debug section collapsed on fresh start.
- [x] **Selected-info placeholder.** Empty selected-tile section shows
  muted "Select a hex for details" instead of blank space.
  - Verify: screenshot before any click.
- [x] **Tuck the skip row away.** Skip/Go/Until/Skip-Until collapse into
  one "Skip…" popover button next to Save/Load (the full row appears
  only inside the popover). Keeps casual players from parsing dev
  machinery every session.
  - Verify: screenshot — top bar shows Save, Load, Skip…, ↻ only.

## P2 — Trade screen (`screens/trade.rs`)

- [x] **"Minor auto-buy" becomes a checkbox** (it currently reads as a
  label, state ambiguity) using `widgets::spawn_checkbox`.
- [x] **Unify the filter bar.** "Commodities 0/11 ▼", "Resources (11)",
  "Countries 0/22 ▼", "Great Powers (6)", "Minor Powers (16)" — same
  widget style (dropdowns), one row, consistent widths; counts formatted
  the same way.
- [x] **Explain the GP column (CC-4)** — header tooltip + replace the dot
  with a small flag or "GP" badge.
- [x] **Sell-slider context.** Show max = current stock in the row label
  ("Timber ×10 in stock"), so the slider range is meaningful.
- Verify (all): `M7_DEBUG=trade` before/after screenshots.

## P2 — Diplomacy screen (`screens/diplomacy.rs`)

- [x] **Disable action buttons until a nation is selected**; enable per
  eligibility (e.g. Break Treaty only with an existing treaty). Disabled
  buttons keep tooltips explaining why (CC-3).
- [x] **Label the standing bar** — "Standing with <Nation>" once selected;
  hide it before selection.
- [x] **Diplomacy-specific side panel.** While in Diplomacy mode, hide the
  generic UI/Debug toggle sections; show only the legend + relation
  details for the hovered/selected nation.
- Verify (all): `M8_DEBUG=diplomacy` screenshot; a `diploselect` script
  variant that selects a nation first.

## P2 — Tech screen (`screens/tech.rs`)

- [x] **Show the full tech timeline.** All techs — available, adopted, and
  future/locked (grayed, with availability year and cost) — grouped by
  category or decade, so the screen is a planning view instead of two
  rows in a void.
- [x] **Rename "Free" → "Adopt (free)"**; paid ones "Adopt ($N)".
- Verify: `M8_DEBUG=tech` screenshot shows the full 1815–1915 tech list
  with locked entries.

## P2 — Setup flow (`setup/ui.rs`, `setup/capital.rs`)

- [x] **Capital suggestions must be distinguishable.** Rows currently
  repeat one town name ("Kiotdargrad ×4").
  - [x] Per-row detail: compass direction from province center +
    coastal/inland + the yield split ("Grain 6 · Fruit 4 · Livestock 2").
  - [x] Hovering a row highlights the hex on the map.
  - [x] Clicking a row pans to and selects the hex.
  - Verify: `M10_DEBUG=capital` screenshot; rows visibly distinct.
- [x] **Group the preview sliders.**
  - [x] "World shape" group: Land amount, Sea ring, Coastline falloff,
    River sources.
  - [x] "Terrain mix" group: the 7 biomes, with a live sum indicator
    (they are normalized — make that visible). Cluster knobs grouped
    under "Clustering".
  - Verify: `M10_DEBUG=preview` screenshot.
- [x] **Label the header zoom buttons** (+/−) with tooltips; switch the
  Terrain/Political text pair to the tab widget.
  - Verify: `M10_DEBUG=preview` screenshot.
- [x] **Rename the "NOI" difficulty** to a player-facing name (e.g.
  "Brutal") — codebase jargon leaking into the UI.
  - Verify: `M10_DEBUG=config` screenshot.

## P3 — Remaining polish

- [x] **Title screen "Continue" button** above New Game: loads the newest
  file in `./saves/` directly; hidden when no saves exist.
  (`intro.rs`; reuse `setup::jobs::start_load`.)
  - Verify: `INTRO_DEBUG=1` screenshot with and without a saves dir.
- [x] **News: coalesce repeated headlines.** N identical "X held back from
  war with Y" / "FINANCIAL CRISIS: …" lines merge into one summary line
  ("5 nations face bankruptcy") with the detail list behind an expander
  or tooltip. This is a view-model-level grouping in `screens/news.rs`.
  - Verify: `M9_DEBUG=news` screenshot — no more than one line per event
    type per turn.
- [x] **News: label the colored edge bars** (category tooltip) on headlines
  (currently unexplained red/blue/black); label the empty top-right
  search box ("Filter…" placeholder).
  - Verify: `M9_DEBUG=news` screenshot.
- [x] **Battles polish.**
  - [x] Minimap name clipping ("CET…") — shrink label font or clamp
    label position inside the minimap bounds.
  - [x] One-line legend for unit strength bars ("bar = remaining
    strength") and rank stars.
  - Verify: `M9_DEBUG=battles` screenshot.
- [x] **Ledger: zebra rows + neutral deltas** (part of the CC-2 audit);
  slightly larger text in the expanded player cash-flow detail.
  - Verify: `M8_DEBUG=ledger` screenshot.
- [x] **Setup config: two-column layout** so Nations count/sliders sit
  above the fold at 720p.
  - Verify: `M10_DEBUG=config` screenshot — no scrollbar at 1280×720
    default window, or Nations visible without scrolling.


## P2 — Group 6: alignment, spacing & interface-scale defaults

Source: alignment/wasted-space re-examination of every screen (2026-07-12),
including previously unreviewed views (Legend, Trade history tabs, Ledger
cash-flow tab, proposal modal).

- [x] **UI scale: "normal" becomes 175%, scalable beyond.** The old max
  (1.75) is the new `DEFAULT_SCALE` (fresh installs + Ctrl+0 reset);
  `MAX_SCALE` rises to 2.5 so scaling UP from the new normal works. The
  side-panel slider range follows automatically; persisted values clamp.
  - [x] Add a `UI_SCALE` env override (screenshot/debug hook) so captures
    can pin a scale without touching `settings.json`.
  - Verify: fresh start (no settings.json) screenshot at 175%; slider
    shows 175% and drags to 250%.
- [x] **Scale-invariant alignment.** Bevy's `UiScale` multiplies every Px
  uniformly, so intra-panel alignment must not change with font size;
  verify by capturing Industry/Trade/Ledger at 80% / 175% / 250% and
  comparing proportions. Fitting regressions found by this audit are the
  items below (clipping is a layout bug, not a scale bug).
  - Verify: `UI_SCALE=0.8|1.75|2.5` screenshot triplets.
- [x] **Ledger tables clip at the right edge.** The WORKERS (Economy tab)
  and RECONCILE (Cash-flow tab) columns sit half under the scrollbar at
  every scale. Reserve a scrollbar gutter on the table content.
  - Verify: `M8_DEBUG=ledger` and `M8_DEBUG=ledgerflow` screenshots show
    the full last column.
- [x] **Table kit: vertical centering + numeric alignment.** Text cells sit
  top-aligned next to taller Buy buttons (Trade offers); numeric columns
  (Avail, Price, Bought, Cost, Sold, Revenue, Qty) are left-aligned under
  their headers. Center rows vertically in `widgets/table.rs` and add
  `ColumnSpec::numeric()` (right-aligned header + cells); narrow the GP
  column (0.5 → 0.35 fr) so it stops reading as dead space.
  - Verify: `M7_DEBUG=trade` + `histdata` screenshots — text centered
    against buttons, numbers right-aligned.
- [x] **Trade history "•" GP dots missed in Group 4.** The aggregated
  Historical Country rows still mark Great-Power partners with a bare
  gold dot ("Cetdaaria •"); replace with the same "GP" badge used in the
  offers table.
  - Verify: `M7_DEBUG=histdata` screenshot.
- [x] **Trade filter bar fits one row.** (chips renamed GPs/Minors with full-name tooltips) "Minor Powers (16)" wraps onto a
  lone second line; compact dropdown widths (170 → 150) and chip
  font/padding so the Orders filter bar is a single row at 1280 px.
  - Verify: `M7_DEBUG=trade` screenshot.
- [x] **Transport steppers align as a column.** Rows without a shortfall
  ("Livestock") let flex push −/count/+ to the right; reserve a
  fixed-width trailing slot for the "N short" label so the steppers sit
  at the same x on every row.
  - Verify: `M7_DEBUG=transport` screenshot — one vertical stepper
    column.
- [x] **Solid chrome over the map.** Map labels ghost through the
  translucent side panel, top tab bar, convenience bar, and setup preview
  sidebar (e.g. "CENTRAL EAST-CENTRAL OCEAN" behind the UI toggles).
  Switch map-adjacent chrome to the solid panel background.
  - Verify: `HUMAN_GAME=1` + `M10_DEBUG=preview` screenshots — no text
    bleed-through.
- [x] **Modal ✕ sits on the dialog, not the screen.** The close button is
  parented to the full-screen overlay, so it floats at the window's
  top-right corner (visible in the proposal screenshot). Move it into the
  dialog's title bar.
  - Verify: `M8_DEBUG=proposal` screenshot.
- [x] **Setup config: Nations above the fold at 1280×720.** (scenario removal + wrapping rows freed the space; wide rows wrap inside columns) The
  Group-5 layout wraps back to one column (2 × 430 min-width + gap
  exceeds the panel's inner width); lower the column min-width so
  Nations lands above the fold as intended.
  - Verify: `M10_DEBUG=config` screenshot — Nations visible without
    scrolling.
- [x] **Burger menu for the convenience bar.** Save / Load / Restart, the
  observer View dropdown, and the skip machinery all hide behind one "☰"
  button (glyph added to the patched pixel font) so the map top-left
  stays clean; triggering any action closes the menu.
  - Verify: `HUMAN_GAME=1` screenshot — bar shows only ☰; a scripted
    open shows the menu.
- [x] **Remove the non-functional scenario cards.** Only the random map
  generator is playable; Congress of Vienna / Concert of Europe / Year
  of Revolutions / Scramble for Africa were dead UI. The Scenario picker
  section and its plumbing are removed (config always uses the random
  generator; scenario support returns when real scenarios exist).
  - Verify: `M10_DEBUG=config` screenshot — no Scenario section.
- [x] **Uniform full-screen headers.** Title sizes and Close buttons vary
  (Industry 17 px and no Close button; Battles 20 px; Tech/Trade/Ledger
  19 px). Standardize: 19 px bold gold title, same header padding, and a
  "Close (Esc)" button on every full-screen overlay including Industry.
  - Verify: header screenshots across Industry/Trade/Tech/Ledger/Battles
    line up.

---

## Execution order (group tracking)

Each group is one batch of changes done and reviewed together. Per the
project workflow: run `/adversarial-review` per group, check off the
items above as they land, and deliver the mandatory before/after
screenshot pair for every change.

- [x] **Group 1** — P1 Industry + P1 Transport (most-used screens,
  biggest mess)
- [x] **Group 2** — P1 cross-cutting passes (color audit, dead-end
  hints, chrome)
- [x] **Group 3** — P2 map side panel + skip row
- [x] **Group 4** — P2 Trade / Diplomacy / Tech / Setup
- [x] **Group 5** — P3 polish batch
- [x] **Group 6** — alignment, spacing & interface-scale defaults
