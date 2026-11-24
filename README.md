# Laser Engraving Calibration

For the Creality Falcon2 Pro S 22W with LightBurn.

## Windows desktop application

The desktop application is called **engraving**. It is a Tauri 2 window with a
SolidJS frontend (`ui/`) over the local Rust image-processing core. It runs
without Python or an OpenCV runtime.

### Build

Prerequisites: Node.js with pnpm, the Rust MSVC toolchain, and Visual Studio C++
Build Tools with the Windows SDK.

The frontend must be built before the executable, because Tauri embeds
`ui/dist`.

```powershell
pnpm --dir ui install
pnpm --dir ui build
cargo build --locked --release --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\laser-engraving-optimizer.exe
```

The executable is built on Windows. Do not distribute a debug build as the final
application; the debug build also opens a console window.

### Development

```powershell
pnpm --dir ui install
cargo tauri dev
```

`cargo tauri dev` starts Vite on port 5173 and opens the application window. For
frontend work only, `pnpm --dir ui dev` serves the same interface in a browser
with an in-memory mock of the host, so no Rust build is needed.

### Lasers, presets and engraving

The interface has three areas on the top bar, following the hierarchy
laser → preset → image:

**Lasers** registers your machines. A laser has only a unique name; it carries no
settings. Every preset belongs to exactly one laser, because a calibration is
valid only for that machine.

**Presets** holds the presets of the selected laser:

1. Create a preset, enter a name and an optional note, and set the block size,
   rows, columns, and curve exponent. Defaults are `400`, `6`, `6`, and `1.8`,
   and gamma, CLAHE, and the colour method live here too.
2. Export the calibration gauge as a PNG. Engrave it using the laser settings
   that this preset will represent. A paper print alone does not measure the
   laser's response on your material.
3. Scan the engraving flat-on in even light. Crop and align it to the outer
   grid edges without rotating or stretching individual cells. The scan must
   match the gauge dimensions: `columns × block size` by `rows × block size`
   pixels (`2400 × 2400` with the defaults).
4. Use **Import scan…** to attach the scan to the saved preset. The application
   keeps a managed copy, so moving the original scan does not break the preset.

A laser that still owns presets cannot be deleted, so remove its presets first.

**Engrave** is where images are processed. Each tab is one image with its own
preset choice and its own result, so several images can be prepared side by
side in one window. Pick a preset of the selected laser, choose an input photo,
process it, and save the engraving as a PNG for LightBurn. The same tab exports
the calibrated simulation PNG and the calibration report CSV of the measured and
fitted sample table.

Changing the preset, its settings, or the laser makes the previewed result stale:
save nothing from it and process the photo again.

Input photos may be PNG, JPG/JPEG, WebP, BMP, or TIFF. HEIC and HEIF are not
supported; convert those to one of the supported formats first.

Use a separate preset for each material and laser-setting combination. Keep the
geometry and exponent consistent with the engraved gauge. The application rejects
a scan with different dimensions; it does not correct perspective or align a
photograph automatically.

Give each laser its wattage. The laser is listed as `name 40W`, and a preset's
power percentage shows the resulting wattage in brackets, for example
`35% (14 W)`. Presets also record the speed (mm/s) and the DPI they were
calibrated with; leave those at `0` when you do not know them.

The Presets area shows the imported calibration scan next to the generated
sample gauge at the same scale, so a wrong-size or badly cropped scan is visible
before an image is processed. Replacing a scan moves the previous managed copy
into `archived-scans` in the application data directory, named after the preset
and the replacement time. The trash button on the scan preview archives the
current scan and leaves the preset saved without one; such a preset is marked
with a red `!` in the list and refuses to process until a new scan is imported.
**Open archive** reveals those files and the trash button beside it clears them.

Both previews draw the exact pixels the sampler reads: one box on the engraved
interior of every patch and two thin strips for the local background on either
side. Use it to judge whether the photo sits where the code expects it. Importing
a scan opens a crop editor first. The gauge frame is fixed on screen: drag the
image to pan it, rotate it with the control in the stage corner or by typing an
exact angle, zoom with the `+` and `−` buttons, and nudge it with the arrow keys
(one source pixel, `Shift` plus arrow for ten). The X/Y/W/H readout shows the
source rectangle that will be imported, and the same sampled pixels stay visible
inside the frame. The crop is rescaled to the gauge size on import, so the result
always matches the preset geometry.

Presets and managed scans are stored in
`%LOCALAPPDATA%\LaserEngravingOptimizer`. Back up that entire directory to retain
the settings, their scans and the archived scans. Use one application instance
at a time; concurrent preset editing is not supported. An invalid preset file is
reported rather than silently replaced.

### Check the implementation

```powershell
cargo test --locked
cargo fmt --all -- --check
pnpm --dir ui exec tsc --noEmit
pnpm --dir ui build
```

Reference fixtures cover calibration samples and lookup tables, gauge geometry,
grayscale preparation, and calibrated output. The Rust decolorize implementation
uses a fixed random seed for repeatable output; its samples differ from the
former Python random generator, so independent runs need not produce identical
images. `src/fixtures/python_reference.json` was generated from the retired
Python implementation, which remains available in the repository history.

Software checks do not establish physical engraving quality. Test a small image
before using a new material, laser setting, or calibration preset.

## Which curve exponent to use

- `1.8`: scans of gauges engraved from an older randomized-tone gauge.
- `1.0`: scans of gauges engraved from the current evenly spaced gauge.

Never use exponent `1.0` with an older gauge scan. The exponent belongs to the
preset that owns the scan.

## Adjust image tones

- `Gamma 1.15`: lighter midtones.
- `Gamma 0.85`: darker midtones.
- `CLAHE 0`: disable local contrast.
- `CLAHE 1.5`: mild local contrast.

## 5. Test laser settings

Unverified starting settings: **9800 mm/min**, **4% minimum**, **23% maximum**, low air assist (~**15%**), **5% overscan**, **450 DPI** (~**0.056 mm**), and **Grayscale** mode.

Test **0.08 mm** (~**318 DPI**) with other settings unchanged. Then test maximum and minimum power separately. Change one variable at a time. Software cannot create tones where the laser and material produce no distinct physical response; validate with a small engraving.
