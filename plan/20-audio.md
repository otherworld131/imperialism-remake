# 20 — Audio & Music

## Overview

The original features classical-style music (19th-century chamber and orchestral) and
extensive sound effects (battles, telegraphs, occupation). The remake should match this
quality level with modern audio production.

## Checklist

### Audio Engine
- [ ] Implement `AudioEngine` trait in domain boundary
- [ ] Adapter for game framework's audio system
- [ ] Support BGM (background music) with crossfade transitions
- [ ] Support SFX (sound effects) with spatial positioning (optional for hex map)
- [ ] Volume controls: Master, BGM, SFX (independent sliders)
- [ ] Mute toggle
- [ ] Audio settings persisted across sessions
- [ ] Unit tests: audio engine state machine (play, pause, stop, crossfade)

### Background Music
- [ ] **Main menu theme** — stately, 19th-century parlor music feel
- [ ] **Strategic map theme(s)** — multiple tracks, rotated or context-sensitive
- [ ] **Tactical battle theme(s)** — dramatic, military marches or orchestral combat music
- [ ] **Victory fanfare**
- [ ] **Defeat music**
- [ ] **Newspaper theme** — brief jingle or ambient
- [ ] Music loops seamlessly
- [ ] Context-sensitive transitions (map → battle → victory)
- [ ] Crossfade between tracks (no abrupt cuts)

### Sound Effects
- [ ] **Battle SFX**: cannon fire, rifle shots, cavalry charge, explosions, fort destruction
- [ ] **Naval SFX**: broadside, ship creaking, waves
- [ ] **Map SFX**: railroad construction, mine operation, factory production
- [ ] **UI SFX**: button clicks, screen transitions, turn end, notification chimes
- [ ] **Diplomatic SFX**: telegraph sound (treaty/war), embassy establishment
- [ ] **Newspaper SFX**: paper rustle, printing press
- [ ] **Occupation/conquest SFX**: march, flag raising
- [ ] All SFX use pooling to avoid allocation during gameplay

### Audio Assets
- [ ] Source or commission royalty-free classical/orchestral music
- [ ] Source or create SFX (Foley, synthesized, or licensed packs)
- [ ] Audio format: OGG Vorbis for music, WAV for short SFX (or framework-preferred)
- [ ] Consistent loudness normalization across all assets
- [ ] File naming convention: `bgm_menu.ogg`, `sfx_cannon_fire.wav`, etc.

### Accessibility
- [ ] All critical game information conveyed visually, not just aurally
- [ ] Subtitles/captions for any narrative audio (newspaper readout, etc.)
- [ ] Audio cues supplement but never replace visual feedback

### Verification Strategy
- [ ] **Unit tests**: Audio engine state machine tests pass
- [ ] **Integration test**: Start game → verify music plays → transition to battle → verify track changes
- [ ] **Smoke test**: Every SFX trigger point fires the correct sound (click through all UI elements)
- [ ] **Performance test**: Playing 10 simultaneous SFX → no frame drops or audio artifacts
- [ ] **Settings test**: Change volume → verify audio levels adjust; mute → verify silence; restart → verify settings persisted
