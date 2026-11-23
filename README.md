# Laser Engraving Calibration

For the Creality Falcon2 Pro S 22W with LightBurn.

## Windows desktop application

The Rust desktop application runs locally without Python or an OpenCV runtime.
Keep the Python scripts below as a reference while validating the port against
your material and laser.

### Build

Prerequisites: the Rust MSVC toolchain and Visual Studio C++ Build Tools with the
Windows SDK.

```powershell
cargo build --locked --release --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\laser-engraving-optimizer.exe
```

The executable is built on Windows. Do not distribute a debug build as the final
application.

### Calibration and presets

1. Select **New preset**, enter a name and an optional note, and set the block size,
   rows, columns, and curve exponent. Defaults are `400`, `6`, `6`, and `1.8`.
   Use **Save preset** before importing a scan or choosing an input photo.
2. Export the calibration gauge as a PNG. Engrave it using the laser settings
   that this preset will represent. A paper print alone does not measure the
   laser's response on your material.
3. Scan the engraving flat-on in even light. Crop and align it to the outer
   grid edges without rotating or stretching individual cells. The scan must
   match the gauge dimensions: `columns × block size` by `rows × block size`
   pixels (`2400 × 2400` with the defaults).
4. Use **Choose calibration scan…** to import the scan into the saved preset.
   The application keeps a managed copy, so moving the original scan does not
   break the preset.
5. Choose an input photo, process it, inspect the result, and save the engraving
   image as a PNG for LightBurn.

Use a separate preset for each material and laser-setting combination.
Keep the geometry and exponent consistent with the engraved gauge. The
application rejects a scan with different dimensions; it does not correct
perspective or align a photograph automatically.

Presets and managed scans are stored in
`%LOCALAPPDATA%\LaserEngravingOptimizer`. Back up that entire directory to retain
both the settings and their scans. Use one application instance at a time;
concurrent preset editing is not supported. An invalid preset file is reported
rather than silently replaced.

### Check the Rust implementation

```powershell
cargo test --locked
cargo fmt --all -- --check
```

Reference fixtures cover calibration samples and lookup tables, gauge geometry,
grayscale preparation, and calibrated output. The Rust decolorize implementation
uses a fixed random seed for repeatable output; its samples differ from Python's
random generator, so independent runs need not produce identical images.
Fixtures can be regenerated with the Python reference environment using
`python src/fixtures/generate_reference.py` from the repository root with
`PYTHONPATH=.`. Python is not needed to run the Rust tests or executable.

Software checks do not establish physical engraving quality. Test a small image
before using a new material, laser setting, or calibration preset.

## Python reference workflow

### Set up (Windows)

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
