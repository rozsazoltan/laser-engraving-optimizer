//! Tauri host: bridges the SolidJS UI to the Rust processing core.
//!
//! Every command returns the fresh [`UiState`] (except `process_photo`, which
//! returns its own result) and the same value is emitted on the `state` event,
//! so the UI can update from either channel. Heavy work runs on the async
//! runtime thread, never on the webview thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{GrayImage, RgbImage};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::calibration::{Calibration, GridCalibrationSpecification};
use crate::presets::{Preset, PresetStore};
use crate::processing::{self, ColorMethod, ProcessingSettings};

const PREVIEW_EDGE: u32 = 640;

/// Calibration geometry as exchanged with the UI.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UiCalibrationSpec {
    pub block_size: u32,
    pub rows: u32,
    pub columns: u32,
    pub exponent: f64,
}

impl From<&GridCalibrationSpecification> for UiCalibrationSpec {
    fn from(spec: &GridCalibrationSpecification) -> Self {
        Self {
            block_size: spec.block_size,
            rows: spec.rows,
            columns: spec.columns,
            exponent: spec.exponent,
        }
    }
}

impl From<&UiCalibrationSpec> for GridCalibrationSpecification {
    fn from(spec: &UiCalibrationSpec) -> Self {
        Self {
            block_size: spec.block_size,
            rows: spec.rows,
            columns: spec.columns,
            exponent: spec.exponent,
        }
    }
}

/// Processing settings as exchanged with the UI.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UiProcessingSettings {
    pub gamma: f64,
    pub clahe_clip: f64,
    pub color_method: String,
}

impl From<&ProcessingSettings> for UiProcessingSettings {
    fn from(settings: &ProcessingSettings) -> Self {
        Self {
            gamma: settings.gamma,
            clahe_clip: settings.clahe_clip,
            color_method: match settings.color_method {
                ColorMethod::Decolorize => "Decolorize",
                ColorMethod::Luma => "Luma",
            }
            .to_string(),
        }
    }
}

impl UiProcessingSettings {
    fn to_settings(&self) -> Result<ProcessingSettings, String> {
        let color_method = match self.color_method.as_str() {
            "Decolorize" => ColorMethod::Decolorize,
            "Luma" => ColorMethod::Luma,
            other => return Err(format!("Unknown color method: {other}")),
        };
        let settings = ProcessingSettings {
            gamma: self.gamma,
            clahe_clip: self.clahe_clip,
            color_method,
        };
        settings.validate()?;
        Ok(settings)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UiLaser {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UiPreset {
    pub id: String,
    pub laser_id: String,
    pub name: String,
    pub note: String,
    pub calibration: UiCalibrationSpec,
    pub processing: UiProcessingSettings,
    #[serde(default)]
    pub has_scan: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UiState {
    pub lasers: Vec<UiLaser>,
    pub presets: Vec<UiPreset>,
    pub selected_laser: Option<String>,
    pub selected_preset: Option<String>,
    pub busy: bool,
    pub status: String,
    pub error: Option<String>,
    pub app_version: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcessResult {
    pub preview_data_url: String,
    pub simulation_data_url: String,
    pub width: u32,
    pub height: u32,
    pub report_csv: String,
}

struct ProcessCache {
    preset_id: String,
    engraving: GrayImage,
    simulation: GrayImage,
    report: String,
}

struct Inner {
    store: PresetStore,
    busy: bool,
    status: String,
    error: Option<String>,
    cache: HashMap<String, ProcessCache>,
}

pub struct Host {
    app: tauri::AppHandle,
    inner: Mutex<Inner>,
}

fn build_state(inner: &Inner) -> UiState {
    let lasers = inner
        .store
        .lasers()
        .iter()
        .map(|laser| UiLaser {
            id: laser.id.clone(),
            name: laser.name.clone(),
        })
        .collect();
    let presets = inner
        .store
        .presets()
        .iter()
        .map(|preset| UiPreset {
            id: preset.id.clone(),
            laser_id: preset.laser_id.clone(),
            name: preset.name.clone(),
            note: preset.note.clone(),
            calibration: UiCalibrationSpec::from(&preset.calibration),
            processing: UiProcessingSettings::from(&preset.processing),
            has_scan: inner.store.calibration_photo_path(preset).is_some(),
        })
        .collect();
    UiState {
        lasers,
        presets,
        selected_laser: inner.store.selected_laser().map(|laser| laser.id.clone()),
        selected_preset: inner.store.selected().map(|preset| preset.id.clone()),
        busy: inner.busy,
        status: inner.status.clone(),
        error: inner.error.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

impl Host {
    pub fn new(app: tauri::AppHandle) -> Result<Self, String> {
        let store = PresetStore::open_default().map_err(|error| error.to_string())?;
        Ok(Self {
            app,
            inner: Mutex::new(Inner {
                store,
                busy: false,
                status: "Ready".to_string(),
                error: None,
                cache: HashMap::new(),
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn publish(&self, inner: &mut Inner) -> UiState {
        let state = build_state(inner);
        let _ = self.app.emit("state", &state);
        state
    }

    fn fail(&self, inner: &mut Inner, message: String) -> String {
        inner.busy = false;
        inner.error = Some(message.clone());
        self.publish(inner);
        message
    }
}

fn guard_busy(inner: &Inner) -> Result<(), String> {
    if inner.busy {
        Err("Another operation is still running.".to_string())
    } else {
        Ok(())
    }
}

fn tab_key(window: &tauri::WebviewWindow, tab: &str) -> String {
    format!("{}:{}", window.label(), tab)
}

fn save_png(image: &GrayImage, path: &Path) -> Result<(), String> {
    image::DynamicImage::ImageLuma8(image.clone())
        .save(path)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))
}

fn preview_image(image: &GrayImage) -> GrayImage {
    let longest = image.width().max(image.height());
    if longest <= PREVIEW_EDGE {
        return image.clone();
    }
    let scale = PREVIEW_EDGE as f64 / longest as f64;
    let width = ((image.width() as f64 * scale).round() as u32).max(1);
    let height = ((image.height() as f64 * scale).round() as u32).max(1);
    image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
}

fn data_url(image: &GrayImage) -> Result<String, String> {
    let mut buffer = Vec::new();
    image::DynamicImage::ImageLuma8(image.clone())
        .write_to(
            &mut std::io::Cursor::new(&mut buffer),
            image::ImageFormat::Png,
        )
        .map_err(|error| format!("Could not encode preview: {error}"))?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&buffer)))
}

fn selected_scan(
    inner: &Inner,
) -> Result<
    (
        String,
        GridCalibrationSpecification,
        ProcessingSettings,
        PathBuf,
    ),
    String,
> {
    let preset = inner
        .store
        .selected()
        .ok_or_else(|| "Select a preset first.".to_string())?;
    let scan = inner
        .store
        .calibration_photo_path(preset)
        .ok_or_else(|| "This preset has no calibration scan yet.".to_string())?;
    Ok((
        preset.id.clone(),
        preset.calibration.clone(),
        preset.processing.clone(),
        scan,
    ))
}

fn load_calibration(
    spec: &GridCalibrationSpecification,
    scan: &Path,
) -> Result<Calibration, String> {
    let image: RgbImage = processing::load_rgb(scan)?;
    let (width, height) = spec.dimensions()?;
    if image.width() != width || image.height() != height {
        return Err(format!(
            "Calibration scan must be exactly {width}x{height} pixels; got {}x{}.",
            image.width(),
            image.height()
        ));
    }
    Calibration::from_scan(spec, &image)
}

#[tauri::command]
pub fn get_state(host: State<'_, Host>) -> UiState {
    build_state(&host.lock())
}

#[tauri::command]
pub fn add_laser(host: State<'_, Host>, name: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.add_laser(&name) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Laser added".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn rename_laser(host: State<'_, Host>, id: String, name: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.rename_laser(&id, &name) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.status = "Laser renamed".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn remove_laser(host: State<'_, Host>, id: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.remove_laser(&id) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Laser removed".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn select_laser(host: State<'_, Host>, id: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.select_laser(&id) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Laser selected".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn select_preset(host: State<'_, Host>, id: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.select(&id) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Preset selected".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn add_preset(
    host: State<'_, Host>,
    name: String,
    laser_id: String,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.add(Preset::new(name, laser_id)) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Preset added".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn save_preset(host: State<'_, Host>, preset: UiPreset) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;

    let calibration = GridCalibrationSpecification::from(&preset.calibration);
    if let Err(error) = calibration.validate() {
        return Err(host.fail(&mut inner, error));
    }
    let settings = match preset.processing.to_settings() {
        Ok(settings) => settings,
        Err(error) => return Err(host.fail(&mut inner, error)),
    };
    let existing = match inner
        .store
        .presets()
        .iter()
        .find(|existing| existing.id == preset.id)
    {
        Some(existing) => existing,
        None => return Err(host.fail(&mut inner, "Preset not found.".to_string())),
    };
    if existing.laser_id != preset.laser_id {
        return Err(host.fail(
            &mut inner,
            "A preset cannot move to another laser.".to_string(),
        ));
    }
    let updated = Preset {
        id: preset.id,
        laser_id: existing.laser_id.clone(),
        name: preset.name,
        note: preset.note,
        calibration,
        processing: settings,
        calibration_photo: existing.calibration_photo.clone(),
    };
    if let Err(error) = inner.store.update(updated) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Preset saved".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn remove_preset(host: State<'_, Host>, id: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.remove(&id) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Preset removed".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn import_scan(host: State<'_, Host>, id: String, path: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.import_calibration_photo(&id, Path::new(&path)) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.status = "Calibration scan imported".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn export_gauge(host: State<'_, Host>, id: String, path: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    let preset = match inner.store.presets().iter().find(|preset| preset.id == id) {
        Some(preset) => preset,
        None => return Err(host.fail(&mut inner, "Preset not found.".to_string())),
    };
    let gauge = match preset.calibration.generate() {
        Ok(gauge) => gauge,
        Err(error) => return Err(host.fail(&mut inner, error)),
    };
    if let Err(error) = save_png(&gauge, Path::new(&path)) {
        return Err(host.fail(&mut inner, error));
    }
    inner.status = "Calibration image exported".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn process_photo(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    tab: String,
    path: String,
) -> Result<ProcessResult, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.busy = true;
    inner.error = None;
    inner.status = "Processing photo".to_string();
    host.publish(&mut inner);

    let outcome = (|| -> Result<(String, GrayImage, GrayImage, String, u32, u32), String> {
        let (preset_id, spec, settings, scan_path) = selected_scan(&inner)?;
        let calibration = load_calibration(&spec, &scan_path)?;
        let photo = processing::load_rgb(Path::new(&path))?;
        let processed = processing::process_photo(&photo, &calibration, &settings)?;
        let report = calibration.report_csv();
        let width = processed.engraving.width();
        let height = processed.engraving.height();
        Ok((
            preset_id,
            processed.engraving,
            processed.simulation,
            report,
            width,
            height,
        ))
    })();

    let (preset_id, engraving, simulation, report, width, height) = match outcome {
        Ok(values) => values,
        Err(error) => return Err(host.fail(&mut inner, error)),
    };

    let result = match (
        data_url(&preview_image(&engraving)),
        data_url(&preview_image(&simulation)),
    ) {
        (Ok(preview), Ok(simulation)) => ProcessResult {
            preview_data_url: preview,
            simulation_data_url: simulation,
            width,
            height,
            report_csv: report.clone(),
        },
        (Err(error), _) | (_, Err(error)) => return Err(host.fail(&mut inner, error)),
    };

    inner.cache.insert(
        tab_key(&window, &tab),
        ProcessCache {
            preset_id,
            engraving,
            simulation,
            report,
        },
    );
    inner.busy = false;
    inner.status = format!("Processed {width}x{height} engraving");
    host.publish(&mut inner);
    Ok(result)
}

fn current_preset(inner: &Inner) -> Option<String> {
    inner.store.selected().map(|preset| preset.id.clone())
}

fn write_cached(
    host: &Host,
    window: &tauri::WebviewWindow,
    tab: &str,
    path: &str,
    label: &str,
    pick: fn(&ProcessCache) -> GrayImage,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    let key = tab_key(window, tab);
    let cached = inner
        .cache
        .get(&key)
        .map(|cache| (cache.preset_id.clone(), pick(cache)));
    let image = match cached {
        Some((preset_id, image)) => {
            if current_preset(&inner).as_deref() != Some(preset_id.as_str()) {
                return Err(host.fail(
                    &mut inner,
                    "The result belongs to a different preset. Process the photo again."
                        .to_string(),
                ));
            }
            image
        }
        None => {
            return Err(host.fail(
                &mut inner,
                "Process a photo before saving its output.".to_string(),
            ))
        }
    };
    if let Err(error) = save_png(&image, Path::new(path)) {
        return Err(host.fail(&mut inner, error));
    }
    inner.status = format!("{label} saved");
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn save_engraving(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    tab: String,
    path: String,
) -> Result<UiState, String> {
    write_cached(&host, &window, &tab, &path, "Engraving", |cache| {
        cache.engraving.clone()
    })
}

#[tauri::command]
pub fn save_simulation(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    tab: String,
    path: String,
) -> Result<UiState, String> {
    write_cached(&host, &window, &tab, &path, "Simulation", |cache| {
        cache.simulation.clone()
    })
}

#[tauri::command]
pub fn save_report(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    tab: String,
    path: String,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    let key = tab_key(&window, &tab);
    let cached = inner
        .cache
        .get(&key)
        .map(|cache| (cache.preset_id.clone(), cache.report.clone()));
    let report = match cached {
        Some((preset_id, report)) => {
            if current_preset(&inner).as_deref() != Some(preset_id.as_str()) {
                return Err(host.fail(
                    &mut inner,
                    "The result belongs to a different preset. Process the photo again."
                        .to_string(),
                ));
            }
            report
        }
        None => {
            return Err(host.fail(
                &mut inner,
                "Process a photo before saving its report.".to_string(),
            ))
        }
    };
    if let Err(error) = std::fs::write(Path::new(&path), report) {
        return Err(host.fail(&mut inner, format!("Could not write {path}: {error}")));
    }
    inner.status = "Calibration report saved".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn pick_open_file(title: String, extensions: Vec<String>) -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    for extension in &extensions {
        dialog = dialog.add_filter(extension.to_uppercase(), &[extension.as_str()]);
    }
    dialog
        .pick_file()
        .await
        .map(|file| file.path().to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn pick_save_file(title: String, file_name: String, extension: String) -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(extension.to_uppercase(), &[extension.as_str()])
        .save_file()
        .await
        .map(|file| file.path().to_string_lossy().into_owned())
}

#[tauri::command]
pub fn minimize_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn toggle_maximize_window(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.is_maximized().map_err(|error| error.to_string())? {
        window.unmaximize().map_err(|error| error.to_string())
    } else {
        window.maximize().map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub fn close_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

/// Registers the host state; called from `tauri::Builder::setup`.
pub fn init(app: &tauri::App) -> Result<(), String> {
    let host = Host::new(app.handle().clone())?;
    app.manage(host);
    Ok(())
}
