import { DEFAULT_CALIBRATION, DEFAULT_PROCESSING, presetKey, type Laser, type Preset, type ProcessResult, type State } from "./types";

/**
 * Browser-only stand-in for the Tauri host, used by `pnpm dev` so the UI can be
 * reviewed without the desktop shell. It keeps lasers, presets and the per-tab
 * result cache in memory, and rejects with the same plain-string messages the
 * host uses.
 */

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

const PHOTO_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];
const SCAN_EXTENSIONS = ["png", "jpg", "jpeg"];

const extensionOf = (path: string) => {
  const match = /\.([a-z0-9]+)$/i.exec(path.trim());
  return match ? match[1].toLowerCase() : "";
};

/** Dot field that reads like an engraving preview. */
function halftone(ink: string, paper: string, seed: number): string {
  const width = 480;
  const height = 320;
  const columns = 60;
  const rows = 40;
  const stepX = width / columns;
  const stepY = height / rows;
  let dots = "";
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < columns; x++) {
      const wave =
        Math.sin((x + seed) / 6.5) * 0.5 +
        Math.cos((y - seed) / 5.2) * 0.35 +
        Math.sin((x + y + seed) / 11) * 0.15;
      const radius = Math.max(0.4, (0.5 - Math.abs(wave) * 0.45) * Math.min(stepX, stepY));
      dots += `<circle cx="${((x + 0.5) * stepX).toFixed(1)}" cy="${((y + 0.5) * stepY).toFixed(1)}" r="${radius.toFixed(2)}"/>`;
    }
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}"><rect width="${width}" height="${height}" fill="${paper}"/><g fill="${ink}">${dots}</g></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

const REPORT_CSV = [
  "block,row,column,exponent,gamma,clahe_clip",
  "0,0,0,1.80,1.00,4.0",
  "1,0,1,1.80,1.00,4.0",
  "2,1,0,1.80,1.00,4.0",
  "3,1,1,1.80,1.00,4.0",
].join("\n");

const preset = (id: string, laserId: string, name: string, note: string, hasScan: boolean): Preset => ({
  id,
  laser_id: laserId,
  name,
  note,
  calibration: { ...DEFAULT_CALIBRATION },
  processing: { ...DEFAULT_PROCESSING },
  has_scan: hasScan,
});

let lasers: Laser[] = [
  { id: "laser-k40", name: "K40 CO₂ 40 W" },
  { id: "laser-diode", name: "Diode 10 W (450 nm)" },
  { id: "laser-fiber", name: "Fiber 20 W" },
];

let presets: Preset[] = [
  preset("preset-birch", "laser-k40", "Birch plywood 3 mm", "Module 40 W, 254 dpi baseline", true),
  preset("preset-mdf", "laser-k40", "MDF 4 mm fill", "Gauge printed, scan still missing", false),
  preset("preset-basswood", "laser-diode", "Basswood 2 mm", "Air assist on, 3 passes", true),
  preset("preset-slate", "laser-diode", "Slate coaster", "Low power, matte finish", true),
];

let selectedLaser: string | null = lasers[0]?.id ?? null;
let selectedPreset: string | null = presets[0]?.id ?? null;
let status = "Ready";
let error: string | null = null;
let sequence = 0;
let photoSequence = 0;

const PHOTO_NAMES = ["sample-carving.png", "sample-owl.png", "sample-sign.png", "sample-portrait.png"];

type CacheEntry = { key: string; result: ProcessResult };

/** process_photo cache, scoped by the tab string the UI sends. */
const cache = new Map<string, CacheEntry>();

const snapshot = (): State => ({
  lasers: structuredClone(lasers),
  presets: structuredClone(presets),
  selected_laser: selectedLaser,
  selected_preset: selectedPreset,
  busy: false,
  status,
  error,
  app_version: "0.1.0 · browser preview",
});

const laserOf = (id: string): Laser => {
  const match = lasers.find((item) => item.id === id);
  if (!match) throw `No laser with id “${id}”.`;
  return match;
};

const presetOf = (id: string): Preset => {
  const match = presets.find((item) => item.id === id);
  if (!match) throw `No preset with id “${id}”.`;
  return match;
};

const currentPreset = (): Preset => {
  if (!selectedPreset) throw "Select a preset before using the engraving tools.";
  return presetOf(selectedPreset);
};

const validateLaserName = (name: string, exceptId?: string) => {
  const trimmed = name.trim();
  if (!trimmed) throw "Laser name cannot be empty.";
  if (trimmed.length > 64) throw "Laser name must be at most 64 characters.";
  if (lasers.some((item) => item.id !== exceptId && item.name.toLowerCase() === trimmed.toLowerCase())) {
    throw `A laser named “${trimmed}” already exists.`;
  }
  return trimmed;
};

const validatePreset = (incoming: Preset) => {
  const current = presetOf(incoming.id);
  if (incoming.laser_id !== current.laser_id) throw "A preset cannot move to another laser.";
  const name = incoming.name.trim();
  if (!name) throw "Preset name cannot be empty.";
  if (presets.some((item) => item.id !== incoming.id && item.laser_id === incoming.laser_id && item.name.toLowerCase() === name.toLowerCase())) {
    throw `A preset named “${name}” already exists on this laser.`;
  }
  const { block_size, rows, columns, exponent } = incoming.calibration;
  if (!(block_size >= 32 && block_size <= 4096)) throw "Block size must be between 32 and 4096 px.";
  if (!(rows >= 2 && rows <= 64)) throw "Rows must be between 2 and 64.";
  if (!(columns >= 2 && columns <= 64)) throw "Columns must be between 2 and 64.";
  if (!(exponent >= 0.1 && exponent <= 8)) throw "Exponent must be between 0.1 and 8.";
  const { gamma, clahe_clip } = incoming.processing;
  if (!(gamma >= 0.1 && gamma <= 4)) throw "Gamma must be between 0.1 and 4.";
  if (!(clahe_clip >= 0 && clahe_clip <= 16)) throw "CLAHE clip must be between 0 and 16.";
};

const firstPresetOf = (laserId: string | null) =>
  presets.find((item) => item.laser_id === laserId)?.id ?? null;

export async function mockCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await wait(command === "get_state" ? 160 : 110);

  switch (command) {
    case "get_state":
      return snapshot() as T;

    case "add_laser": {
      const name = validateLaserName(String(args?.name ?? ""));
      const created: Laser = { id: `laser-${++sequence}`, name };
      lasers = [...lasers, created];
      selectedLaser = created.id;
      selectedPreset = null;
      status = `Added “${name}”`;
      error = null;
      return snapshot() as T;
    }

    case "rename_laser": {
      const id = String(args?.id ?? "");
      laserOf(id);
      const name = validateLaserName(String(args?.name ?? ""), id);
      lasers = lasers.map((item) => (item.id === id ? { ...item, name } : item));
      status = `Renamed laser to “${name}”`;
      error = null;
      return snapshot() as T;
    }

    case "remove_laser": {
      const id = String(args?.id ?? "");
      const match = laserOf(id);
      const owned = presets.filter((item) => item.laser_id === id);
      if (owned.length > 0) {
        throw `“${match.name}” still owns ${owned.length} preset${owned.length === 1 ? "" : "s"}. Remove those presets first.`;
      }
      lasers = lasers.filter((item) => item.id !== id);
      if (selectedLaser === id) {
        selectedLaser = lasers[0]?.id ?? null;
        selectedPreset = firstPresetOf(selectedLaser);
      }
      status = `Removed “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "select_laser": {
      const id = String(args?.id ?? "");
      const match = laserOf(id);
      selectedLaser = id;
      selectedPreset = firstPresetOf(id);
      status = `Laser “${match.name}” selected`;
      error = null;
      return snapshot() as T;
    }

    case "select_preset": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      selectedPreset = id;
      selectedLaser = match.laser_id;
      status = `Preset “${match.name}” selected`;
      error = null;
      return snapshot() as T;
    }

    case "add_preset": {
      const laserId = String(args?.laserId ?? "");
      const laser = laserOf(laserId);
      const name = String(args?.name ?? "").trim();
      if (!name) throw "Preset name cannot be empty.";
      if (presets.some((item) => item.laser_id === laserId && item.name.toLowerCase() === name.toLowerCase())) {
        throw `A preset named “${name}” already exists on this laser.`;
      }
      const created = preset(`preset-${++sequence}`, laserId, name, "", false);
      presets = [...presets, created];
      selectedLaser = laser.id;
      selectedPreset = created.id;
      status = `Created “${created.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "save_preset": {
      const incoming = args?.preset as Preset | undefined;
      if (!incoming) throw "The preset payload was missing.";
      const current = presetOf(incoming.id);
      validatePreset(incoming);
      const next: Preset = {
        ...current,
        name: incoming.name.trim(),
        note: incoming.note,
        calibration: { ...incoming.calibration },
        processing: { ...incoming.processing },
      };
      presets = presets.map((item) => (item.id === next.id ? next : item));
      status = `Saved “${next.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "remove_preset": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      presets = presets.filter((item) => item.id !== id);
      if (selectedPreset === id) selectedPreset = firstPresetOf(match.laser_id);
      cache.clear();
      status = `Removed “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "import_scan": {
      const id = String(args?.id ?? "");
      const path = String(args?.path ?? "");
      const match = presetOf(id);
      if (!SCAN_EXTENSIONS.includes(extensionOf(path))) {
        throw "Calibration scan must be a PNG or JPEG image.";
      }
      presets = presets.map((item) => (item.id === id ? { ...item, has_scan: true } : item));
      cache.clear();
      status = `Calibration scan imported for “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "export_gauge": {
      const id = String(args?.id ?? "");
      const path = String(args?.path ?? "");
      const match = presetOf(id);
      status = `Gauge for “${match.name}” written to ${path}`;
      error = null;
      return snapshot() as T;
    }

    case "process_photo": {
      const tab = String(args?.tab ?? "");
      const path = String(args?.path ?? "");
      if (!tab) throw "The engraving request had no tab id.";
      const active = currentPreset();
      if (!active.has_scan) throw `“${active.name}” has no calibration scan. Import one first.`;
      if (!PHOTO_EXTENSIONS.includes(extensionOf(path))) {
        throw `Unsupported image format: “${path}”. Use PNG, JPEG, WebP or TIFF.`;
      }
      await wait(650);
      const result: ProcessResult = {
        preview_data_url: halftone("#aab5c6", "#0b0b0b", 3),
        simulation_data_url: halftone("#d8c5a8", "#191410", 7),
        width: 1600,
        height: 1067,
        report_csv: REPORT_CSV,
      };
      cache.set(tab, { key: presetKey(active), result: structuredClone(result) });
      status = `Processed 1600 × 1067 px with “${active.name}”`;
      error = null;
      return result as T;
    }

    case "save_engraving":
    case "save_simulation":
    case "save_report": {
      const tab = String(args?.tab ?? "");
      const path = String(args?.path ?? "");
      const entry = cache.get(tab);
      if (!entry) throw "This tab has no processed result yet. Process an image first.";
      if (entry.key !== presetKey(currentPreset())) {
        throw "This tab’s result belongs to a different preset. Process the image again.";
      }
      const kind = command === "save_engraving" ? "Engraving" : command === "save_simulation" ? "Simulation" : "Report";
      status = `${kind} saved to ${path}`;
      error = null;
      return snapshot() as T;
    }

    case "pick_open_file": {
      const title = String(args?.title ?? "");
      if (/scan|calibration|gauge/i.test(title)) return "C:\\mock\\scans\\gauge.png" as T;
      const name = PHOTO_NAMES[photoSequence++ % PHOTO_NAMES.length];
      return `C:\\mock\\photos\\${name}` as T;
    }

    case "pick_save_file": {
      const fileName = String(args?.fileName ?? "export");
      return `C:\\mock\\exports\\${fileName}` as T;
    }

    case "minimize_window":
    case "toggle_maximize_window":
    case "close_window":
      return undefined as T;

    default:
      throw `Unknown command: ${command}`;
  }
}
