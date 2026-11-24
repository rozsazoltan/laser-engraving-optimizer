import { DEFAULT_CALIBRATION, DEFAULT_PROCESSING, presetKey, type Laser, type Preset, type PresetPreview, type ProcessResult, type ScanGeometry, type ScanSource, type State } from "./types";

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
  speed: 0,
  power_percent: 0,
  dpi: 0,
  has_scan: hasScan,
});

let lasers: Laser[] = [
  { id: "laser-k40", name: "K40 CO₂", watts: 40 },
  { id: "laser-diode", name: "Diode (450 nm)", watts: 10 },
  { id: "laser-fiber", name: "Fiber", watts: 20 },
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
  if (!(incoming.speed >= 0 && incoming.speed <= 10000)) throw "Speed must be between 0 and 10000 mm/s.";
  if (!(incoming.power_percent >= 0 && incoming.power_percent <= 100)) throw "Power must be between 0 and 100%.";
  if (!(incoming.dpi >= 0 && incoming.dpi <= 10000)) throw "DPI must be between 0 and 10000.";
};

const previewImage = (label: string, width: number, height: number) => {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}"><rect width="100%" height="100%" fill="#171717"/><path d="M0 ${height * .25}H${width}M0 ${height * .5}H${width}M0 ${height * .75}H${width}M${width * .25} 0V${height}M${width * .5} 0V${height}M${width * .75} 0V${height}" stroke="#8491a5" stroke-width="${Math.max(2, width / 180)}"/><text x="50%" y="52%" fill="#eee" font-size="${Math.max(20, width / 16)}" text-anchor="middle" font-family="sans-serif">${label}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
};

const scanSource = (label: string, width: number, height: number): ScanSource => ({
  width,
  height,
  data_url: previewImage(label, width, height),
});
const rotatedSize = (width: number, height: number, angle: number) => {
  const radians = angle * Math.PI / 180;
  let cosine = Math.abs(Math.cos(radians));
  let sine = Math.abs(Math.sin(radians));
  if (cosine < 1e-12) cosine = 0;
  if (sine < 1e-12) sine = 0;
  return {
    width: Math.max(1, Math.ceil(width * cosine + height * sine)),
    height: Math.max(1, Math.ceil(width * sine + height * cosine)),
  };
};
const managedScans = new Map<string, ScanSource>();
const originalScans = new Map<string, ScanSource>();
for (const item of presets.filter((entry) => entry.has_scan)) {
  managedScans.set(item.id, scanSource("Current scan", 2400, 2400));
  originalScans.set(item.id, scanSource("Original scan", 3200, 2400));
}

const scanGeometry = (match: Preset): ScanGeometry => {
  const block = match.calibration.block_size;
  const width = block * match.calibration.columns;
  const height = block * match.calibration.rows;
  const count = match.calibration.rows * match.calibration.columns - 1;
  const spacing = block * 0.1;
  const safety = (block - spacing) * 0.3;
  const patches = Array.from({ length: count + 1 }, (_, index) => {
    const n = index;
    const col = n % match.calibration.columns;
    const row = Math.floor(n / match.calibration.columns);
    const factor = n === 0 ? 0 : (n / count) ** match.calibration.exponent;
    const x0 = Math.ceil(col * block + spacing);
    const y0 = Math.ceil(row * block + spacing + safety * factor);
    const x1 = Math.ceil((col + 1) * block - spacing);
    const y1 = Math.ceil((row + 1) * block - spacing);
    const inset = Math.max(1, Math.round((x1 - x0) * 0.015));
    const end = Math.max(inset + 1, Math.round((x1 - x0) * 0.075));
    const px0 = x0 + Math.round((x1 - x0) * 0.15);
    const px1 = x1 - Math.round((x1 - x0) * 0.15);
    const py0 = y0 + Math.round((y1 - y0) * 0.2);
    const py1 = y1 - Math.round((y1 - y0) * 0.2);
    const rect = (left: number, top: number, right: number, bottom: number) => ({
      x: left, y: top, width: right - left, height: bottom - top,
    });
    return {
      level: Math.round(255 - factor * 255),
      engraved: rect(px0, py0, px1, py1),
      left: rect(x0 - end, py0, x0 - inset, py1),
      right: rect(x1 + inset, py0, x1 + end, py1),
    };
  });
  return { width, height, patches };
};

const firstPresetOf = (laserId: string | null) =>
  presets.find((item) => item.laser_id === laserId)?.id ?? null;

export async function mockCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await wait(command === "get_state" ? 160 : 110);
  const operationDelay: Record<string, number> = {
    process_photo: 750,
    scan_source: 450,
    scan_source_preset: 450,
    import_scan_crop: 550,
    import_scan_crop_preset: 550,
    export_gauge: 500,
    save_engraving: 450,
    save_simulation: 450,
    save_report: 450,
    open_scan_archive: 400,
    clear_scan_archive: 400,
    pick_open_file: 400,
    pick_save_file: 350,
  };
  if (operationDelay[command]) await wait(operationDelay[command]);

  switch (command) {
    case "get_state":
      return snapshot() as T;

    case "add_laser": {
      const name = validateLaserName(String(args?.name ?? ""));
      const watts = Number(args?.watts ?? 0);
      if (!(watts >= 0 && watts <= 1000)) throw "Laser watts must be between 0 and 1000.";
      const created: Laser = { id: `laser-${++sequence}`, name, watts };
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
      const watts = Number(args?.watts ?? 0);
      if (!(watts >= 0 && watts <= 1000)) throw "Laser watts must be between 0 and 1000.";
      lasers = lasers.map((item) => (item.id === id ? { ...item, name, watts } : item));
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
        speed: incoming.speed,
        power_percent: incoming.power_percent,
        dpi: incoming.dpi,
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

    case "scan_geometry":
      return scanGeometry(presetOf(String(args?.id ?? ""))) as T;

    case "scan_source": {
      const path = String(args?.path ?? "");
      if (!SCAN_EXTENSIONS.includes(extensionOf(path))) throw "Calibration scan must be a PNG or JPEG image.";
      const source: ScanSource = { width: 1600, height: 1200, data_url: previewImage("Source scan", 1600, 1200) };
      return source as T;
    }

    case "scan_source_preset": {
      const match = presetOf(String(args?.id ?? ""));
      if (!match.has_scan) throw "This preset has no calibration scan.";
      return (managedScans.get(match.id) ?? scanSource("Current scan", 2400, 2400)) as T;
    }

    case "scan_source_original": {
      const match = presetOf(String(args?.id ?? ""));
      if (!match.has_scan) throw "This preset has no calibration scan.";
      const current = managedScans.get(match.id) ?? scanSource("Current scan", 2400, 2400);
      return (originalScans.get(match.id) ?? current) as T;
    }

    case "import_scan_crop": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      const path = String(args?.path ?? "");
      const sourceWidth = 1600;
      const sourceHeight = 1200;
      const angle = Number(args?.angle ?? 0);
      if (!Number.isFinite(angle) || angle < -180 || angle > 180) throw "Rotation must be between -180 and 180 degrees.";
      const canvas = rotatedSize(sourceWidth, sourceHeight, angle);
      const x = Number(args?.x);
      const y = Number(args?.y);
      const width = Number(args?.width);
      const height = Number(args?.height);
      if (!SCAN_EXTENSIONS.includes(extensionOf(path))) throw "Calibration scan must be a PNG or JPEG image.";
      if (![x, y, width, height].every(Number.isInteger) || x < 0 || y < 0 || width < 32 || height < 32 || x + width > canvas.width || y + height > canvas.height) {
        throw "The crop is outside the source image or smaller than 32 × 32 px.";
      }
      presets = presets.map((item) => item.id === id ? { ...item, has_scan: true } : item);
      managedScans.set(id, scanSource("Current scan", match.calibration.block_size * match.calibration.columns, match.calibration.block_size * match.calibration.rows));
      if (!originalScans.has(id)) originalScans.set(id, scanSource("Original scan", sourceWidth, sourceHeight));
      cache.clear();
      status = `Calibration scan imported for “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "import_scan_crop_preset": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      if (!match.has_scan) throw "This preset has no calibration scan.";
      const source = managedScans.get(id) ?? scanSource("Current scan", 2400, 2400);
      const angle = Number(args?.angle ?? 0);
      if (!Number.isFinite(angle) || angle < -180 || angle > 180) throw "Rotation must be between -180 and 180 degrees.";
      const canvas = rotatedSize(source.width, source.height, angle);
      const x = Number(args?.x), y = Number(args?.y), width = Number(args?.width), height = Number(args?.height);
      if (![x, y, width, height].every(Number.isInteger) || x < 0 || y < 0 || width < 32 || height < 32 || x + width > canvas.width || y + height > canvas.height) {
        throw "The crop is outside the source image or smaller than 32 × 32 px.";
      }
      managedScans.set(id, scanSource("Current scan", match.calibration.block_size * match.calibration.columns, match.calibration.block_size * match.calibration.rows));
      cache.clear();
      status = `Calibration scan imported for “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "import_scan_crop_source": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      if (!match.has_scan) throw "This preset has no calibration scan.";
      const source = originalScans.get(id) ?? managedScans.get(id);
      if (!source) throw "This preset has no calibration scan.";
      const angle = Number(args?.angle ?? 0);
      if (!Number.isFinite(angle) || angle < -180 || angle > 180) throw "Rotation must be between -180 and 180 degrees.";
      const canvas = rotatedSize(source.width, source.height, angle);
      const x = Number(args?.x), y = Number(args?.y), width = Number(args?.width), height = Number(args?.height);
      if (![x, y, width, height].every(Number.isInteger) || x < 0 || y < 0 || width < 32 || height < 32 || x + width > canvas.width || y + height > canvas.height) {
        throw "The crop is outside the source image or smaller than 32 × 32 px.";
      }
      managedScans.set(id, scanSource("Current scan", match.calibration.block_size * match.calibration.columns, match.calibration.block_size * match.calibration.rows));
      cache.clear();
      status = `Calibration scan imported for “${match.name}”`;
      error = null;
      return snapshot() as T;
    }

    case "remove_scan": {
      const id = String(args?.id ?? "");
      const match = presetOf(id);
      if (!match.has_scan) throw "This preset has no calibration scan.";
      presets = presets.map((item) => (item.id === id ? { ...item, has_scan: false } : item));
      cache.clear();
      status = `Calibration scan of “${match.name}” moved to the archive`;
      error = null;
      return snapshot() as T;
    }

    case "preset_preview": {
      const match = presetOf(String(args?.id ?? ""));
      const width = match.calibration.block_size * match.calibration.columns;
      const height = match.calibration.block_size * match.calibration.rows;
      const result: PresetPreview = {
        scan_data_url: match.has_scan ? previewImage("Calibration scan", width, height) : null,
        gauge_data_url: previewImage("Sample gauge", width, height),
        width,
        height,
      };
      return result as T;
    }

    case "open_scan_archive":
      status = "Opened the archived scans folder.";
      error = null;
      return snapshot() as T;

    case "clear_scan_archive":
      status = "Archived scans cleared.";
      error = null;
      return snapshot() as T;

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
