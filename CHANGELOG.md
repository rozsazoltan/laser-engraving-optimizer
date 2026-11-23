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
- Calibration diagnostics in `calibration_report.csv` and `calibration_curve.png`.
- Calibration report CSV and simulation PNG export from the desktop application.
- Optional `--curve-exponent`, `--gamma`, `--clahe`, and `--color-method` parameters.
- Optional linear tone spacing for calibration gauges (`--curve-exponent 1.0`).
- Creality Falcon2 Pro S 22W calibration instructions.

### Changed
- Simulator uses the same fitted response as the optimizer.
- Replaced the Python runtime and OpenCV dependency with a self-contained Rust build.

### Removed
- Python reference scripts, `requirements.txt`, and the Python toolchain pin;
  committed numerical fixtures preserve their behaviour as test references.

### Fixed
- Incorrect whole-image white reference affected by illumination gradient.
- White-reference sample rectangle extending into engraved cells.
- Unstable cubic polynomial fitting and non-monotonic power interpolation.
- Loss of almost all test patches caused by an arbitrary brightness-difference threshold.
