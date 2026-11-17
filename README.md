# Laser Engraving Optimizer — calibrated grayscale edition

Upgraded [LaserEngravingOptimizer](https://github.com/laurenzse/LaserEngravingOptimizer) for **variable-power grayscale engraving on wood**, including Creality Falcon2 Pro S 22W + LightBurn. Old command-line positional arguments still work.

## What changed

- Measure **each engraved square against local, unengraved wood margins** rather than one white reference for whole sheet. Reduces camera lighting and wood-grain gradients.
- Replace brittle point cutoff (`white_difference > 10`), polynomial inverse fitting, and Savitzky–Golay truncation with a **monotone, background-corrected calibration curve** (isotonic regression + interpolated inverse). Flat regions do not create negative or nonsensical power trends.
- Simulate by applying **exact same forward calibration** used to optimize input. A gray simulation represents estimated *absolute brightness* of photographed wood, not contrast-stretched artistic preview.
- Default photo preprocessing is deterministic luminance grayscale with optional CLAHE (local contrast); legacy `decolorize` retained as explicit option.
- Export `calibration_report.csv` (all 35 measured points) and `calibration_curve.png` after each run.
- Add independent `--curve-exponent` control. Existing 6×6 gauge uses **1.8**; recommended *new* 6×6 gauge uses **1.0** to distribute sample levels more uniformly.
- Reduce requirements to installed libraries actually used.

## Install

Use Python 3.11 or newer; on Windows:

```powershell
mise use python@3.11
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

## Optimize a photo

Use a scanned calibration gauge and an input photo:

```powershell
.\.venv\Scripts\python.exe optimize_color_photo.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg"
```

Outputs: `greyscale.png`, `greyscale_for_engraving.png` **(import this in LightBurn)**, `engraving_simulation_result.png`, `calibration_report.csv`, `calibration_curve.png`.

## Create a calibration gauge

```powershell
.\.venv\Scripts\python.exe create_calibration_image.py 400 6 6 --curve-exponent 1.0
```

This writes `calibration_gauge.png` to the working directory. No generated root PNG gauges are included in this repository. When scanning a gauge, use the same exponent in all commands:

```powershell
.\.venv\Scripts\python.exe optimize_color_photo.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg" --curve-exponent 1.0
.\.venv\Scripts\python.exe simulate_engraving.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg" --curve-exponent 1.0
```

Grayscale source must remain **6×6 in same orientation**. Square #1 (top left) is **unengraved**. Scan must be square, cropped *exactly* to full 6×6 region and resized to **2400×2400 pixels**. Shoot flat on, in diffuse even light with exposure/white balance fixed; flatbed scanner is preferred. Avoid auto enhancement, HDR, strong reflections, rotated/misaligned crops.

### LightBurn process

1. Import gauge; set to **60×60 mm**, giving 10×10 mm per block. Same physical size and laser focus for subsequent images.
2. Choose Image Mode **Grayscale** (NOT Jarvis/Stucki). Set engraving speed, min/max power, interval/DPI, overscan and air assist. Keep all settings identical for calibration and final output. Keep one pass.
3. First find a dark square that is dark, not excessively charred. For 22W diode, too much energy can *decrease* darkness by charring/ablating wood, causing saturation/inverted response.
4. Existing user baseline: **9800 mm/min, 4% min, 23% max, air assist on low, 5% overscan, 450 DPI (~0.056 mm interval)**. These are observed settings, **not verified optimum**. Try isolated A/B test of line interval **0.08 mm (~318 DPI)** and max power/speed separately. For grayscale ensure white pixels are laser-off; investigate whether min 4% is a usable engraving threshold.
5. Import `greyscale_for_engraving.png`. Disable further LightBurn image corrections and keep grayscale, same settings and engraving scale assumptions. Preview cannot predict wood chemistry or camera artifacts perfectly.

**No code can create physically distinct tones where laser/material response is flat.** Unmarked first-row squares are expected when near-white source values fall under wood burn threshold. A corrected algorithm avoids incorrectly treating them as bad measurements, but cannot invent actual burn response. If the wood only supports a handful of stable darkness levels, experiment with a *separate* black/white dithering workflow (requires different test/calibration and LightBurn image mode).

## Optional tonal controls

```powershell
.\.venv\Scripts\python.exe optimize_color_photo.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg" --gamma 1.15 --clahe 1.5
```

`--gamma > 1`: lighter midtones. `--gamma < 1`: darker midtones. `--clahe 0`: disable local contrast enhancement. `--color-method decolorize`: use original randomized color-sensitive algorithm (may change result each run). Defaults: `--gamma 1.0 --clahe 2.0 --color-method luma`.

`calibration_curve.png`: red dots = observed local darkness; blue line = monotonically fitted response. Dark plateau (line nearly flat at right) = power increase yields little useful extra shade. `calibration_report.csv`: values in approximate grayscale units; camera-dependent, not spectrophotometric.

## Known limitations

- Background subtraction handles gradual illumination gradients, **not** perspective distortion, misregistration, different timber, smoke streaks, changing laser energy, or deep wood grain.
- Calibration fitted from single gauge, not replicated independent exposures; its prediction is an estimate. Visually confirm with a small portrait crop before engraving large photos.
- Grayscale PNG must be engraved in LightBurn **Grayscale** with same min/max power as gauge. Algorithm does not generate G-code or directly control laser power.
