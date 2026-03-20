# 27 — Build, CI/CD & Release

## Overview

Automated build pipeline ensures every commit is buildable, testable, and releasable.
Initial target: Windows. Architecture supports future macOS/Linux builds.

## Checklist

### Build System
- [ ] Build tool: `cargo build` — single command
- [ ] Build configurations: Debug (dev symbols, assertions) and Release (optimized, no debug)
- [ ] Build script: `build.ps1` / `build.sh` — clean build from scratch
- [ ] Build all projects in dependency order
- [ ] Copy data files (definitions, sprites, audio, localization) to output directory
- [ ] Build produces a self-contained distributable folder
- [ ] Build time target: < 60 seconds from clean state
- [ ] Unit tests: build script exits 0 on success, nonzero on failure

### CI Pipeline (GitHub Actions / Azure DevOps)
- [ ] **On every commit / PR**:
  - [ ] Checkout code
  - [ ] Restore dependencies
  - [ ] Build (Release configuration)
  - [ ] Run unit tests (Domain + Application)
  - [ ] Run architecture tests (dependency rules)
  - [ ] Run integration tests
  - [ ] Code formatting check (`cargo fmt --check`)
  - [ ] Linting / static analysis (`cargo clippy` — zero warnings policy)
  - [ ] Generate test coverage report → enforce thresholds
  - [ ] Upload test results as artifacts
- [ ] **Nightly**:
  - [ ] All above +
  - [ ] Simulation tests (100-turn, AI-only games)
  - [ ] Performance benchmarks → compare to baseline
  - [ ] Memory leak check (extended play simulation)
  - [ ] Build installers for Windows
- [ ] **On tag/release**:
  - [ ] Full pipeline +
  - [ ] Build signed release binaries
  - [ ] Build installer (MSI via `wix` crate, or NSIS, or `cargo-bundle`)
  - [ ] Create GitHub Release with changelog and binaries
  - [ ] Publish to distribution platform (if applicable)

### Release Packaging — Windows
- [ ] Self-contained executable (Rust compiles to native binary — no runtime prerequisite)
- [ ] Installer: MSI via `wix` crate, or NSIS, or `cargo-bundle` with: install directory selection, desktop shortcut, Start menu entry, uninstaller
- [ ] Portable option: zip file, runs from any directory
- [ ] Minimum OS: Windows 10 (x64)
- [ ] Signed binaries (code signing certificate)
- [ ] Runtime dependencies bundled (native libraries, if any)

### Release Packaging — Future Platforms
- [ ] macOS: `.app` bundle, possibly notarized for Gatekeeper
- [ ] Linux: AppImage or Flatpak
- [ ] Web: WASM build (Rust has first-class `wasm32` target support)

### Versioning
- [ ] Semantic versioning: MAJOR.MINOR.PATCH
- [ ] Version embedded in binary (displayed in main menu)
- [ ] Git tags for releases: `v1.0.0`, `v1.1.0`, etc.
- [ ] Changelog maintained: `CHANGELOG.md` with Keep a Changelog format
- [ ] Save file version linked to game version (migration support)

### Verification Strategy
- [ ] **Build test**: `build.sh` / `build.ps1` → exits 0, produces runnable binary
- [ ] **CI test**: Push a commit → CI pipeline runs → all green
- [ ] **Installer test**: Install on clean Windows 10 VM → game launches → menu renders
- [ ] **Portable test**: Unzip portable package → run exe → game launches
- [ ] **Upgrade test**: Install v1.0.0, save game → upgrade to v1.1.0 → load save → works
- [ ] **Uninstall test**: Uninstall → verify no files left behind (except saves in user directory)
