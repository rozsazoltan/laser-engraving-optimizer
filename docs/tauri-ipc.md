# Tauri IPC contract

Contract between the SolidJS frontend (`ui/`) and the Rust host (`src/host.rs`).
Both sides must implement exactly this document. The host is the only owner of
paths and persisted state.

## Domain model

```
Laser 1..n ──< Preset 1..n ──< Engraving job (one per workspace tab)
```

A calibration is valid only for the laser it was measured on, therefore:

- every preset belongs to exactly one laser (`laser_id`, required);
- a preset may only be used while its laser exists;
- a laser that still has presets cannot be removed;
- preset geometry settings (`calibration`, `processing`) are stored per preset;
- image generation is a separate area from preset management.

## Shared types

```ts
type Laser = {
  id: string;
  name: string; // 1..64 chars, unique (case-insensitive)
  watts: number; // 0..1000; 0 means not recorded yet
};

type CalibrationSpec = {
  block_size: number; // 32..4096
  rows: number; // 2..64
  columns: number; // 2..64
  exponent: number; // 0.1..8.0, default 1.8
};

type ColorMethod = "Decolorize" | "Luma";

type ProcessingSettings = {
  gamma: number; // 0.1..4.0, default 1.0
  clahe_clip: number; // 0.0..16.0, default 4.0
  color_method: ColorMethod; // default "Decolorize"
};

type Preset = {
  id: string;
  laser_id: string;
  name: string;
  note: string;
  calibration: CalibrationSpec;
  processing: ProcessingSettings;
  speed: number; // mm/s, 0..10000; 0 means not recorded yet
  power_percent: number; // 0..100, 0 means not recorded yet
  dpi: number; // 0..10000; 0 means not recorded yet
  has_scan: boolean;
};

type State = {
  lasers: Laser[];
  presets: Preset[];
  selected_laser: string | null;
  selected_preset: string | null;
  busy: boolean;
  status: string | null;
  error: string | null;
  app_version: string;
};

type ProcessResult = {
  preview_data_url: string; // PNG data URL, long edge <= 640 px
  simulation_data_url: string; // PNG data URL, long edge <= 640 px
  width: number;
  height: number;
  report_csv: string;
};

type PresetPreview = {
  scan_data_url: string | null; // PNG data URL, long edge <= 640 px
  gauge_data_url: string; // PNG data URL, long edge <= 640 px
  width: number; // gauge pixel width
  height: number; // gauge pixel height
};
```

Defaults for a new laser: a name; `watts = 0` (unrecorded).
Laser display label: `` `${name} ${watts}W` `` — a space between the name and the
number (`K40 40W`). When `watts` is 0 the label is just the name.
Defaults for a new preset: `calibration = { block_size: 400, rows: 6, columns: 6, exponent: 1.8 }`,
`processing = { gamma: 1.0, clahe_clip: 4.0, color_method: "Decolorize" }`, and
`speed`, `power_percent`, `dpi` all 0 (unrecorded).
Power display: `watts × power_percent / 100`, rendered as `(12.5 W)` right after the
percentage. Omit the parenthetical when `watts` or `power_percent` is 0.

## Commands

Every command returns the fresh `State`, and the host also emits a `state`
event with the same payload. Rejections are plain strings.

### Lasers

| Command | Args | Notes |
| --- | --- | --- |
| `add_laser` | `name`, `watts` | Trimmed name 1..64 chars, unique (case-insensitive); `watts` 0..1000. Becomes selected. |
| `rename_laser` | `id`, `name`, `watts` | Same validation; keeps the selection. |
| `remove_laser` | `id` | Rejected while the laser still owns presets. |

### Presets

| Command | Args | Notes |
| --- | --- | --- |
| `select_laser` | `id` | Filters the preset list; selects that laser's first preset if any. |
| `select_preset` | `id` | |
| `add_preset` | `name`, `laser_id` | Requires an existing laser. |
| `save_preset` | `preset` | Validates id, laser id, ranges, uniqueness. |
| `remove_preset` | `id` | Removes its managed scan copy. |
| `import_scan` | `id`, `path` | Validates the decoded image against the preset's exact gauge size; stores a managed copy. |
| `remove_scan` | `id` | Moves the preset's managed scan into the archive. The preset stays saved without a scan. |
| `export_gauge` | `id`, `path` | Writes the preset's calibration gauge PNG. |
| `preset_preview` | `id` | Returns `PresetPreview`: the stored scan and the generated gauge, both scaled to the same long edge, plus the gauge pixel size. Fetched when a preset is selected or its scan changes; never part of `State`. |
| `scan_geometry` | `id` | Returns `ScanGeometry` for the preset's gauge: pixel size plus every sampled region in gauge coordinates. Needs no stored scan. |
| `scan_source` | `path` | Returns `ScanSource` for an image the user picked: real pixel size and a PNG data URL preview. Used by the crop editor before importing. |
| `scan_source_preset` | `id` | Returns `ScanSource` for the preset's already-saved managed scan, so the crop editor can re-open and refine it. Rejects presets without a scan. |
| `import_scan_crop` | `id`, `path`, `x`, `y`, `width`, `height`, `angle`, `corners?` | Imports the integer crop of the picked image, rescaled to the preset's exact gauge size, as the managed scan. The crop may extend past the source; the missing area is filled with black. Rejects a crop with no overlap and one smaller than 32x32. `corners` replaces the crop rectangle with a perspective warp. |
| `import_scan_crop_preset` | `id`, `x`, `y`, `width`, `height`, `angle`, `corners?` | Crops the preset's already-saved managed scan and resaves the result at the exact gauge size, archiving the previous copy. Used by the editor's edit mode. |
| `open_scan_archive` | — | Opens the archived-scan folder in the OS file manager. |
| `clear_scan_archive` | — | Empties the archived-scan folder. |

Replacing a preset's scan archives the previous managed copy instead of deleting it:
`<preset name> - <timestamp>.png` in `archived-scans/` under the app data dir. The
archive is shared by all presets and survives preset removal.
A preset may also be left without a scan on purpose: `remove_scan` archives the
current copy and keeps the preset saved. Such a preset is incomplete — the UI
marks it in the preset list, and processing rejects it with an explanatory error.

#### Sample regions

The sampler reads three rectangles per gauge patch: the engraved interior and the
two background strips left and right of it. Those rectangles must be produced by a
single source of truth in `src/calibration.rs` and exposed as:

```text
ScanGeometry = {
  width: number, height: number,
  patches: Array<{
    level: number,                 // gauge grey level of the patch
    engraved: Rect, left: Rect, right: Rect
  }>
}
Rect = { x: number, y: number, width: number, height: number }
ScanCrop = { angle: number, x: number, y: number, width: number, height: number, corners?: [number, number][] | null }
ScanSource = { width: number, height: number, data_url: string, crop?: ScanCrop | null }
```

`ScanGeometry` is expressed in gauge pixels, so the UI can draw the overlay on both
the stored scan and the generated gauge at any display scale. `scan_source` returns
the real pixel size of the picked file plus a bounded preview (long edge ≤ 1600 px)
so the crop editor can map display coordinates to source pixels exactly.

All three crop commands store the applied `ScanCrop` on the preset, and
`scan_source_original` returns it in `crop`, so the editor can reopen the original
photo with exactly the rotation, frame and perspective that produced the stored
scan.

The crop editor moves by whole source pixels: arrow keys nudge the crop by 1 source
pixel, and the overlay of `ScanGeometry` is drawn inside the crop rectangle at the
scaled position so the user sees which pixels will be sampled.

### Engraving

Tab-scoped by the caller so several tabs and windows never share results.

| Command | Args | Notes |
| --- | --- | --- |
| `process_photo` | `tab`, `path` | Uses the selected preset of the calling window. Blocks nothing; runs off the webview thread. |
| `save_engraving` | `tab`, `path` | Writes the cached full-resolution engraving. |
| `save_simulation` | `tab`, `path` | Writes the cached simulation. |
| `save_report` | `tab`, `path` | Writes the cached calibration report CSV. |
| `pick_open_file` | `title`, `extensions` | Returns `string \| null`. |
| `pick_save_file` | `title`, `file_name`, `extension` | Returns `string \| null`. |

### Windows

| Command | Notes |
| --- | --- |
| `minimize_window`, `toggle_maximize_window`, `close_window` | Act on the calling window. |
| `get_state` | Initial state for a window. |

## Rules

- The UI never receives or constructs filesystem paths except for a path the
  user just chose through `pick_open_file` / `pick_save_file`.
- `busy` is true while `import_scan`, `export_gauge`, `process_photo`,
  `save_*` run. While busy, the UI disables preset switching, laser switching,
  removal and preset saving; it must not leave a result reachable that belongs
  to a different preset.
- Changing the selected preset, the laser, or the draft geometry/settings
  invalidates the result shown in that tab. Saving is only allowed for the
  still-current result.
- A failed command leaves the persisted store exactly as it was.
- Managed scan copies live in the app data directory; originals may move.
- The host writes nowhere else, except the explicit save/export paths.
- Each window keeps its own tab list; tabs do not sync between windows, but
  every window reads and writes the same laser/preset store, and reacts to
  `state` events published by other windows.
- Store schema is versioned. A store from an older schema is migrated in place
  once (its presets are attached to a generated laser named `Migrated laser`);
  a store from a newer or unknown schema is reported as an error and left
  untouched.
- `%LOCALAPPDATA%\LaserEngravingOptimizer` holds the store; the app assumes a
  single running instance per app data directory.

## Window and shell

- `productName`, window title, and wordmark: `engraving`.
- One window only: 1180x780, minimum 900x600, `decorations: false`. Multiple
  images are worked on as tabs inside this window, never as extra OS windows.
- Frontend dev URL `http://localhost:5173`, built assets `ui/dist`.
- The frontend renders its own chrome: a drag region (`data-tauri-drag-region`),
  a navigation strip, and custom minimize/maximize/close buttons.

### Chrome and tab strip

Modelled on the locally cloned Autark (`d:\github\verzly\autark`,
`ui/src/style.css`): almost no outlines, state carried by fills and one accent
line. Autark itself has no OS-level multi-window — its "window support" is one
window with an app tab strip plus tray hide/show, and `engraving` follows the
same single-window, tabs-only approach.

- Chrome strip: height 36px, `padding: 0 5px 0 8px`, `gap: 4px`, drag region on
  the strip only; every interactive child is `no-drag`.
- Wordmark `engraving` at the start of the strip, then the three area entries as
  tab-style buttons, then flexible space, one borderless `+` (26x26) that adds a
  document tab in the Engrave area, then the window controls.
- Tab: height 29px, radius 6px, `border: 0`, transparent background, text
  `#999`, max width 130px, padding `0 6px`, gap 5px, name ellipsised. Hover
  `#151515` with text `#ddd`; active `#1a1a1a` with text `#eee` and a 1px
  `#9aaac3` line inset 10px from left and right along its top edge. The close
  glyph (16x16, `#999`) appears on hover only.
- Document tabs in the Engrave area reuse the same tab styling, sit in their own
  slim strip above the content surface, scroll horizontally without a visible
  scrollbar, and end with the same 26x26 `+` to add a tab.
- Icon buttons (`+`, gear, window controls) are `border: 0`, transparent,
  colour `#888`, radius 5px; hover `#1a1a1a` with colour `#eee`.
- Window controls sit at the strip's right edge, 34-46px wide, full strip height.

## Layout

Three separate areas, reachable from one flat navigation strip, so a single
window is never crowded while every function stays reachable:

1. **Lasers** – register a laser with a unique name; rename or remove it.
   Nothing else lives here.
2. **Presets** – the presets of the selected laser: name, note, geometry,
   gamma/CLAHE/colour method, calibration scan import, gauge export.
3. **Engrave** – image generation: document tabs (one per image), each tab
   picks one of the presets of the selected laser, then chooses an image,
   processes, previews and saves engraving/simulation/report.

Use few outlines: separate areas with spacing and typography rather than
borders; a border is reserved for the single content surface and for inputs
that need a visible affordance. A preset must never be usable without a laser,
and no area is hidden behind a modal that blocks the whole app.

## Visual tokens

```
--bg      #0a0a0a   window background
--card    #0d0d0d   content surface
--line    #262626   the few remaining borders
--text    #e7e7e7
--muted   #a0a0a0
--faint   #858585
--accent  #a9b8d0
--mono    Consolas, "Cascadia Mono", monospace
```

Font: `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`, body 13px.
Radius 12px, 1px lines, 8px inset. Focus outline `#bac9e2`.

## Startup loading screen

Wordmark `engraving`, muted line `Loading presets…`, and a thin indeterminate
accent bar. Shown until the first `get_state` resolves (minimum ~400 ms), with a
short fade. A failed initial load shows the error plus a Retry action instead of
an empty app.

## Operation feedback

State, persistence, preview, and image commands execute on Tauri blocking
workers. Native file dialogs remain asynchronous, and window controls retain
their UI-thread behavior. IPC success payloads and persistence rules are unchanged.

The frontend tracks an operation from file selection through its command result.
Pending actions have local labels and a live status-bar message. Completion,
cancellation and failure release the action controls. The crop editor displays
pending and error messages inside its dialog, where the status bar is not visible.
Preview requests and image decoding expose separate loading and failure states.

Operation feedback is indeterminate: the commands do not report measured progress,
so the UI does not present elapsed-time estimates as completion percentages.
Unrelated navigation remains available; conflicting mutations cannot overlap.

### Rotation

The three crop commands take an `angle` argument (degrees, -180..180, default 0). The image is rotated about its centre first and the canvas grows to the rotated bounding box: `w' = max(1, ceil(w*|cos| + h*|sin|))`, `h' = max(1, ceil(w*|sin| + h*|cos|))`, with trig values within 1e-12 of zero snapped to zero. The crop rectangle is expressed in that rotated canvas space, so the UI mirrors the same formula. Pixels outside the source use the nearest edge pixel.

### Perspective

The same three crop commands accept an optional `corners` array of four `[x, y]` points in the rotated canvas space, ordered top-left, top-right, bottom-right, bottom-left. When present, the corners replace the crop rectangle: the quadrilateral is warped projectively onto the preset's exact gauge size, so a photo taken at an angle can be straightened. Corners must be finite and enclose at least 4 px². Pixels that map outside the source are filled with black, matching the crop path.
