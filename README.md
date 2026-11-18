# Laser Engraving Calibration

For the Creality Falcon2 Pro S 22W with LightBurn.

## Set up (Windows)

```powershell
mise use python@3.11
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

## 1. Process an existing gauge scan

No new engraving is needed. Old gauges use exponent `1.8`:

```powershell
.\.venv\Scripts\python.exe optimize_color_photo.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg" --curve-exponent 1.8
```

Use `greyscale_for_engraving.png` in LightBurn. The run also creates `greyscale.png`, `engraving_simulation_result.png`, `calibration_report.csv`, and `calibration_curve.png`.

## 2. Make and scan a new linear gauge

Create a gauge with evenly spaced tones:

```powershell
.\.venv\Scripts\python.exe create_calibration_image.py 400 6 6 --curve-exponent 1.0
```

This generates `calibration_gauge.png` and `calibration_gauge_linear.png` in the working directory. Engrave a fresh gauge in LightBurn using **Grayscale**, **60×60 mm**, and one pass. The blank top-left square is the reference.

Scan or photograph the new engraving flat-on in even light. Crop to the exact outer edges of the 6×6 grid, preserve its orientation, and resize to **2400×2400 pixels**. Save as `scanned_gauge_linear.png`.

Process this new scan with exponent `1.0`:

```powershell
.\.venv\Scripts\python.exe optimize_color_photo.py 400 6 6 ".\scanned_gauge_linear.png" ".\input.jpg" --curve-exponent 1.0
```

Never use exponent `1.0` with an old gauge scan.

## 3. Simulate a photo without optimization

Use a gauge scan to simulate the photo directly. This does not optimize it:

```powershell
.\.venv\Scripts\python.exe simulate_engraving.py 400 6 6 ".\scanned_gauge.png" ".\input.jpg"
```

## 4. Adjust image tones

- `--gamma 1.15`: lighter midtones.
- `--gamma 0.85`: darker midtones.
- `--clahe 0`: disable local contrast.
- `--clahe 1.5`: mild local contrast.

## 5. Test laser settings

Unverified starting settings: **9800 mm/min**, **4% minimum**, **23% maximum**, low air assist (~**15%**), **5% overscan**, **450 DPI** (~**0.056 mm**), and **Grayscale** mode.

Test **0.08 mm** (~**318 DPI**) with other settings unchanged. Then test maximum and minimum power separately. Change one variable at a time. Software cannot create tones where the laser and material produce no distinct physical response; validate with a small engraving.

## 6. Run automated tests

```powershell
.\.venv\Scripts\python.exe -m unittest discover -s tests -v
```
