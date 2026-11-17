# Changelog

All notable changes to this modified project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Local wood-background calibration for every engraved gauge patch.
- Monotonic laser-response fitting and stable inverse tone mapping.
- Calibration diagnostics in `calibration_report.csv` and `calibration_curve.png`.
- Optional `--curve-exponent`, `--gamma`, `--clahe`, and `--color-method` parameters.
- Optional linear tone spacing for calibration gauges (`--curve-exponent 1.0`).
- Creality Falcon2 Pro S 22W calibration instructions.

### Changed
- Made luminance grayscale preparation deterministic by default.
- Made simulator use same fitted response as optimizer.
- Simplified dependencies to NumPy and OpenCV.

### Fixed
- Incorrect whole-image white reference affected by illumination gradient.
- White-reference sample rectangle extending into engraved cells.
- Unstable cubic polynomial fitting and non-monotonic power interpolation.
- Loss of almost all test patches caused by an arbitrary brightness-difference threshold.
