/** Shared IPC types and defaults. Mirrors docs/tauri-ipc.md; keep in sync with the Rust host. */

export type Laser = { id: string; name: string };

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
  has_scan: boolean;
};

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
export type Draft = Pick<Preset, "name" | "note" | "calibration" | "processing">;

/** One document tab: its own image, preset choice and processed result. */
export type DocTab = {
  id: string;
  title: string;
  presetId: string | null;
  photoPath: string | null;
  result: ProcessResult | null;
  resultKey: string | null;
};

export type Area = "lasers" | "presets" | "engrave";

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
});

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
  ].join("|");
