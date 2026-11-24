/** Shared IPC types and defaults. Mirrors docs/tauri-ipc.md; keep in sync with the Rust host. */

export type Laser = { id: string; name: string; watts: number };

export type CalibrationSpec = {
  block_size: number; // pixels, 32..4096
  rows: number; // 2..64
  columns: number; // 2..64
  exponent: number; // 0.1..8.0
};

export type ColorMethod = "Decolorize" | "Luma";

export type ProcessingSettings = {
  gamma: number; // 0.1..4.0
  clahe_clip: number; // 0.0..16.0
  color_method: ColorMethod;
};

export type Preset = {
  id: string;
  laser_id: string;
  name: string;
  note: string;
  calibration: CalibrationSpec;
  processing: ProcessingSettings;
  speed: number;
  power_percent: number;
  dpi: number;
  has_scan: boolean;
};

export type PresetPreview = {
  scan_data_url: string | null;
  gauge_data_url: string;
  width: number;
  height: number;
};

export type ScanRect = { x: number; y: number; width: number; height: number };
export type ScanPatch = { level: number; engraved: ScanRect; left: ScanRect; right: ScanRect };
export type ScanGeometry = { width: number; height: number; patches: ScanPatch[] };
export type ScanCrop = {
  angle: number;
  x: number;
  y: number;
  width: number;
  height: number;
  corners?: [number, number][] | null;
};

export type ScanSource = { width: number; height: number; data_url: string; crop?: ScanCrop | null };

export type State = {
  lasers: Laser[];
  presets: Preset[];
  selected_laser: string | null;
  selected_preset: string | null;
  busy: boolean;
  status: string;
  error: string | null;
  app_version: string;
};

export type ProcessResult = {
  preview_data_url: string;
  simulation_data_url: string;
  width: number;
  height: number;
  report_csv: string;
};

/** Editable part of a preset. */
export type Draft = Pick<Preset, "name" | "note" | "calibration" | "processing" | "speed" | "power_percent" | "dpi">;

export type JobEntry = {
  presetId: string;
  presetName: string;
  photoPath: string;
  result: ProcessResult;
  resultKey: string;
  at: number;
};

/** One document tab: its own image, preset choice and processing history. */
export type DocTab = {
  id: string;
  title: string;
  presetId: string | null;
  photoPath: string | null;
  entries: JobEntry[];
  index: number;
};

export type Area = "lasers" | "presets" | "engrave" | "faq";
export type SpeedUnit = "mm/s" | "mm/min";

export const DEFAULT_CALIBRATION: CalibrationSpec = {
  block_size: 400,
  rows: 6,
  columns: 6,
  exponent: 1.8,
};

export const DEFAULT_PROCESSING: ProcessingSettings = {
  gamma: 1,
  clahe_clip: 4,
  color_method: "Decolorize",
};

export const COLOR_METHODS: ColorMethod[] = ["Decolorize", "Luma"];

export const draftOf = (preset: Preset): Draft => ({
  name: preset.name,
  note: preset.note,
  calibration: { ...preset.calibration },
  processing: { ...preset.processing },
  speed: preset.speed,
  power_percent: preset.power_percent,
  dpi: preset.dpi,
});

export const laserLabel = (laser: Laser): string =>
  laser.watts > 0 ? `${laser.name} ${laser.watts}W` : laser.name;

export const powerWatts = (preset: Preset | Draft, laser: Laser | null): number =>
  (laser?.watts ?? 0) * preset.power_percent / 100;

export const formatPower = (percent: number, watts: number): string =>
  percent === 0 ? "—" : watts === 0 ? `${percent}%` : `${percent}% (${Number(watts.toFixed(2))} W)`;

/** A result stays valid only for the saved preset it was produced with. */
export const presetKey = (preset: Preset) =>
  [
    preset.id,
    preset.laser_id,
    preset.has_scan ? "scan" : "no-scan",
    preset.calibration.block_size,
    preset.calibration.rows,
    preset.calibration.columns,
    preset.calibration.exponent,
    preset.processing.gamma,
    preset.processing.clahe_clip,
    preset.processing.color_method,
    preset.speed,
    preset.power_percent,
    preset.dpi,
  ].join("|");
