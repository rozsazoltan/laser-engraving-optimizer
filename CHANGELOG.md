# Changelog

All notable changes to this modified project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Native Rust Windows desktop application for local photo processing, preview,
  full-resolution engraving PNG export, and calibration gauge PNG export.
- Persistent named calibration presets with notes, grid and processing settings,
  and managed calibration scan copies in the user's local application data.
- Rust regression tests for numerical Python-reference comparisons, preset
  persistence and failure safety, and desktop interaction state.
- Local wood-background calibration for every engraved gauge patch.
- Monotonic laser-response fitting and stable inverse tone mapping.
- Calibration report CSV export from the desktop application.
- Tauri 2 desktop window named `engraving` with a SolidJS frontend and a startup
  loading screen.
- Laser registry for the owned machines; every preset belongs to one laser, and a
  laser that still owns presets cannot be removed.
- Separate Lasers, Presets, and Engrave areas, with one document tab per image so
  several images can be prepared in the same window.
- Store schema 2 for lasers and laser-bound presets, with a one-time migration of
  existing presets onto a generated laser.
- Creality Falcon2 Pro S 22W calibration instructions.

### Changed
- Simulator uses the same fitted response as the optimizer.
- Replaced the Python runtime and OpenCV dependency with a self-contained Rust build.
- Desktop frontend moved from the embedded egui interface to a Tauri webview that
  embeds the SolidJS build from `ui/dist`.
- Calibration presets are bound to a laser, and the laser selection scopes which
  presets are visible.

### Removed
- Python reference scripts, `requirements.txt`, and the Python toolchain pin;
  committed numerical fixtures preserve their behaviour as test references.
- `--curve-exponent`, `--gamma`, `--clahe`, and `--color-method` command-line
  parameters, and the Python `calibration_curve.png` output.

### Fixed
- Incorrect whole-image white reference affected by illumination gradient.
- White-reference sample rectangle extending into engraved cells.
- Unstable cubic polynomial fitting and non-monotonic power interpolation.
- Loss of almost all test patches caused by an arbitrary brightness-difference threshold.
