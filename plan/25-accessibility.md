# 25 — Accessibility & Localization

## Overview

A superlative product is accessible to the widest possible audience. This covers visual
accessibility, input alternatives, and localization for multiple languages.

## Checklist

### Visual Accessibility
- [ ] Color-blind modes: Deuteranopia, Protanopia, Tritanopia filters
- [ ] Nation colors distinguishable in all color-blind modes (use patterns/icons as supplements)
- [ ] High-contrast mode: increased border visibility, enhanced text contrast
- [ ] Adjustable font size: small / medium / large / extra-large
- [ ] UI scaling: 100% / 125% / 150% / 200%
- [ ] Minimap uses patterns in addition to colors for nation identification
- [ ] Terrain tiles distinguishable by icon/pattern, not just color
- [ ] Unit tests: color contrast ratios meet WCAG AA standard for all themes

### Input Accessibility
- [ ] Full keyboard navigation for all screens (tab order, arrow keys, hotkeys)
- [ ] Hotkey reference card (accessible in-game)
- [ ] Remappable controls
- [ ] Mouse-only play fully supported (no keyboard-only actions)
- [ ] Keyboard-only play fully supported (no mouse-only actions)
- [ ] Gamepad support (stretch goal — map d-pad to hex navigation)
- [ ] Confirm destructive actions (war declarations, treaty breaking) with dialog
- [ ] Unit tests: keyboard navigation covers all interactive elements

### Screen Reader Support (Stretch Goal)
- [ ] ARIA-equivalent labels for all UI elements
- [ ] Descriptive text for map tiles, units, buildings on focus
- [ ] Battle narration mode: text description of combat events
- [ ] Announcement of turn changes, combat results, diplomatic events

### Audio Accessibility
- [ ] Independent volume controls: Master, BGM, SFX
- [ ] Visual indicators for all audio cues (notifications have both sound and visual flash)
- [ ] Subtitles for any spoken content
- [ ] Option to disable screen shake / flashing effects

### Localization Infrastructure
- [ ] String table system: all UI text from localization files
- [ ] `data/localization/{locale}.json` — key-value pairs
- [ ] Placeholder substitution: `"Turn {turn_number} — Year {year}"`
- [ ] Pluralization support: `"1 unit" / "3 units"`
- [ ] Right-to-left (RTL) text support (stretch goal — Arabic, Hebrew)
- [ ] Date/number formatting per locale
- [ ] Font fallback for extended character sets (CJK, Cyrillic, etc.)
- [ ] Unit tests: all string keys present in all supported locales

### Supported Languages (Initial)
- [ ] English (default, complete)
- [ ] Framework for adding: French, German, Spanish, Portuguese, Russian, Chinese, Japanese
- [ ] Translation contributions: community-editable locale files in mod format
- [ ] Missing translation fallback: display English string + warning icon

### Verification Strategy
- [ ] **Unit tests**: Color contrast, keyboard navigation, locale completeness tests pass
- [ ] **Accessibility audit**: Manual walkthrough of every screen with keyboard-only navigation
- [ ] **Color-blind test**: Enable each color-blind mode → verify all nations distinguishable
- [ ] **Font scaling test**: Set each font size → verify no text overflow or clipping
- [ ] **Locale test**: Switch to each supported locale → verify no missing strings, no layout breaks
- [ ] **Screen reader test** (if implemented): Navigate game with screen reader → verify all elements announced
