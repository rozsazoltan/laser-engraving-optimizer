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

use crate::calibration::{Calibration, GridCalibrationSpecification, SampleRect};
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
    pub watts: f64,
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
    pub speed: f64,
    #[serde(default)]
    pub power_percent: f64,
    #[serde(default)]
    pub dpi: f64,
    #[serde(default)]
    pub has_scan: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresetPreview {
    pub scan_data_url: Option<String>,
    pub gauge_data_url: String,
    pub width: u32,
    pub height: u32,
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
    previews: HashMap<String, PresetPreview>,
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
            watts: laser.watts,
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
            speed: preset.speed,
            power_percent: preset.power_percent,
            dpi: preset.dpi,
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
                previews: HashMap::new(),
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
    preview_image_with_edge(image, PREVIEW_EDGE)
}

fn preview_image_with_edge(image: &GrayImage, edge: u32) -> GrayImage {
    let longest = image.width().max(image.height());
    if longest <= edge {
        return image.clone();
    }
    let scale = edge as f64 / longest as f64;
    let width = ((image.width() as f64 * scale).round() as u32).max(1);
    let height = ((image.height() as f64 * scale).round() as u32).max(1);
    image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanPatch {
    pub level: u8,
    pub engraved: SampleRect,
    pub left: SampleRect,
    pub right: SampleRect,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanGeometry {
    pub width: u32,
    pub height: u32,
    pub patches: Vec<ScanPatch>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanSource {
    pub width: u32,
    pub height: u32,
    pub data_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<crate::presets::ScanCrop>,
}

#[tauri::command]
pub async fn scan_geometry(host: State<'_, Host>, id: String) -> Result<ScanGeometry, String> {
    let inner = host.lock();
    let preset = inner
        .store
        .presets()
        .iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| "Preset not found.".to_string())?;
    build_scan_geometry(&preset.calibration)
}

fn build_scan_geometry(spec: &GridCalibrationSpecification) -> Result<ScanGeometry, String> {
    let (width, height) = spec.dimensions()?;
    let patches = spec
        .sample_regions()?
        .into_iter()
        .map(|region| ScanPatch {
            level: region.level,
            engraved: region.engraved,
            left: region.left,
            right: region.right,
        })
        .collect();
    Ok(ScanGeometry {
        width,
        height,
        patches,
    })
}

fn rotate_expand(image: &RgbImage, angle_deg: f64) -> Result<RgbImage, String> {
    if !angle_deg.is_finite() || !(-180.0..=180.0).contains(&angle_deg) {
        return Err("rotation must be between -180 and 180 degrees".into());
    }
    let mut angle = angle_deg % 360.0;
    if angle <= -180.0 {
        angle += 360.0;
    } else if angle > 180.0 {
        angle -= 360.0;
    }
    if angle.abs() < 1e-9 {
        return Ok(image.clone());
    }

    let radians = angle.to_radians();
    let clean_trig = |value: f64| if value.abs() < 1e-12 { 0.0 } else { value };
    let cos = clean_trig(radians.cos());
    let sin = clean_trig(radians.sin());
    let (width, height) = image.dimensions();
    let expanded_width = ((width as f64 * cos.abs() + height as f64 * sin.abs()).ceil()).max(1.0);
    let expanded_height = ((width as f64 * sin.abs() + height as f64 * cos.abs()).ceil()).max(1.0);
    if expanded_width > crate::calibration::MAX_DIMENSION as f64
        || expanded_height > crate::calibration::MAX_DIMENSION as f64
        || expanded_width * expanded_height > crate::calibration::MAX_PIXELS as f64
    {
        return Err("Rotated image dimensions exceed safe limits.".into());
    }
    let out_width = expanded_width as u32;
    let out_height = expanded_height as u32;
    let source_center_x = width as f64 / 2.0;
    let source_center_y = height as f64 / 2.0;
    let output_center_x = out_width as f64 / 2.0;
    let output_center_y = out_height as f64 / 2.0;

    Ok(RgbImage::from_fn(out_width, out_height, |x, y| {
        let dx = x as f64 + 0.5 - output_center_x;
        let dy = y as f64 + 0.5 - output_center_y;
        let source_x = (cos * dx + sin * dy + source_center_x - 0.5)
            .clamp(0.0, width.saturating_sub(1) as f64);
        let source_y = (-sin * dx + cos * dy + source_center_y - 0.5)
            .clamp(0.0, height.saturating_sub(1) as f64);
        let x0 = source_x.floor() as u32;
        let y0 = source_y.floor() as u32;
        let x1 = (x0 + 1).min(width - 1);
        let y1 = (y0 + 1).min(height - 1);
        let tx = source_x - x0 as f64;
        let ty = source_y - y0 as f64;
        let p00 = image.get_pixel(x0, y0);
        let p10 = image.get_pixel(x1, y0);
        let p01 = image.get_pixel(x0, y1);
        let p11 = image.get_pixel(x1, y1);
        image::Rgb(std::array::from_fn(|channel| {
            let top = p00[channel] as f64 * (1.0 - tx) + p10[channel] as f64 * tx;
            let bottom = p01[channel] as f64 * (1.0 - tx) + p11[channel] as f64 * tx;
            (top * (1.0 - ty) + bottom * ty).round() as u8
        }))
    }))
}

#[tauri::command]
pub async fn scan_source(host: State<'_, Host>, path: String) -> Result<ScanSource, String> {
    let _inner = host.lock();
    drop(_inner);
    scan_source_path(Path::new(&path))
}

fn scan_source_path(path: &Path) -> Result<ScanSource, String> {
    let source = processing::load_rgb(path)?;
    let (width, height) = source.dimensions();
    let preview = image::DynamicImage::ImageRgb8(source).to_luma8();
    let data_url = data_url(&preview_image_with_edge(&preview, 1600))?;
    Ok(ScanSource {
        width,
        height,
        data_url,
        crop: None,
    })
}

fn scan_crop(
    angle: Option<f64>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    corners: &Option<Vec<[f64; 2]>>,
) -> crate::presets::ScanCrop {
    crate::presets::ScanCrop {
        angle: angle.unwrap_or_default(),
        x,
        y,
        width,
        height,
        corners: corners.clone(),
    }
}

fn preset_scan_path(store: &PresetStore, id: &str) -> Result<PathBuf, String> {
    let preset = store
        .presets()
        .iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| "Preset not found.".to_string())?;
    store
        .scan_path(preset)
        .ok_or_else(|| "This preset has no calibration scan.".to_string())
}

#[tauri::command]
pub async fn scan_source_preset(host: State<'_, Host>, id: String) -> Result<ScanSource, String> {
    let path = {
        let inner = host.lock();
        preset_scan_path(&inner.store, &id)?
    };
    scan_source_path(&path)
}

#[tauri::command]
pub async fn scan_source_original(host: State<'_, Host>, id: String) -> Result<ScanSource, String> {
    let (path, crop) = {
        let inner = host.lock();
        let path = original_scan_path(&inner.store, &id)?;
        let crop = inner
            .store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .and_then(|preset| preset.scan_crop.clone());
        (path, crop)
    };
    let mut source = scan_source_path(&path)?;
    source.crop = crop;
    Ok(source)
}

fn original_scan_path(store: &PresetStore, id: &str) -> Result<PathBuf, String> {
    let preset = store
        .presets()
        .iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| "Preset not found.".to_string())?;
    store
        .source_path(preset)
        .or_else(|| store.scan_path(preset))
        .ok_or_else(|| "This preset has no calibration scan.".to_string())
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

fn build_preset_preview(
    spec: &GridCalibrationSpecification,
    scan_path: Option<PathBuf>,
) -> Result<PresetPreview, String> {
    let gauge = spec.generate()?;
    let (width, height) = (gauge.width(), gauge.height());
    // A missing or corrupt scan only hides the scan; the gauge still serves.
    let scan_data_url = scan_path
        .and_then(|path| processing::load_rgb(&path).ok())
        .and_then(|scan| {
            let gray = image::DynamicImage::ImageRgb8(scan).to_luma8();
            data_url(&preview_image(&gray)).ok()
        });
    Ok(PresetPreview {
        scan_data_url,
        gauge_data_url: data_url(&preview_image(&gauge))?,
        width,
        height,
    })
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
    let scan = inner.store.calibration_photo_path(preset).ok_or_else(|| {
        "This preset is incomplete: import a calibration scan before processing.".to_string()
    })?;
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

fn crop_with_padding(source: &RgbImage, x: i64, y: i64, width: u32, height: u32) -> RgbImage {
    let mut out = RgbImage::from_pixel(width, height, image::Rgb([0, 0, 0]));
    let (source_width, source_height) = source.dimensions();
    let left = x.max(0) as u32;
    let top = y.max(0) as u32;
    let right = (x + width as i64).clamp(0, source_width as i64) as u32;
    let bottom = (y + height as i64).clamp(0, source_height as i64) as u32;
    if right > left && bottom > top {
        let part =
            image::imageops::crop_imm(source, left, top, right - left, bottom - top).to_image();
        image::imageops::replace(&mut out, &part, left as i64 - x, top as i64 - y);
    }
    out
}

fn warp_quad(
    source: &RgbImage,
    corners: &[[f64; 2]],
    target_width: u32,
    target_height: u32,
) -> Result<RgbImage, String> {
    if corners.len() != 4 || corners.iter().flatten().any(|value| !value.is_finite()) {
        return Err("Perspective needs four finite corners.".into());
    }
    let (p0, p1, p2, p3) = (corners[0], corners[1], corners[2], corners[3]);
    let area = (p0[0] * p1[1] - p1[0] * p0[1] + p1[0] * p2[1] - p2[0] * p1[1] + p2[0] * p3[1]
        - p3[0] * p2[1]
        + p3[0] * p0[1]
        - p0[0] * p3[1])
        / 2.0;
    if area.abs() < 4.0 {
        return Err("The perspective corners must form a usable quadrilateral.".into());
    }
    // Projective map from the unit square to the quad (Heckbert); its inverse
    // maps every target pixel straight back onto the source image.
    let dx1 = p1[0] - p2[0];
    let dx2 = p3[0] - p2[0];
    let dy1 = p1[1] - p2[1];
    let dy2 = p3[1] - p2[1];
    let sx = p0[0] - p1[0] + p2[0] - p3[0];
    let sy = p0[1] - p1[1] + p2[1] - p3[1];
    let den = dx1 * dy2 - dx2 * dy1;
    if den.abs() < 1e-9 {
        return Err("The perspective corners must form a usable quadrilateral.".into());
    }
    let g = (sx * dy2 - dx2 * sy) / den;
    let h = (dx1 * sy - sx * dy1) / den;
    let a = p1[0] - p0[0] + g * p1[0];
    let b = p3[0] - p0[0] + h * p3[0];
    let d = p1[1] - p0[1] + g * p1[1];
    let e = p3[1] - p0[1] + h * p3[1];
    let (width, height) = source.dimensions();
    let max_x = width.saturating_sub(1) as f64;
    let max_y = height.saturating_sub(1) as f64;
    Ok(RgbImage::from_fn(target_width, target_height, |x, y| {
        let u = (x as f64 + 0.5) / target_width as f64;
        let v = (y as f64 + 0.5) / target_height as f64;
        let divisor = g * u + h * v + 1.0;
        if divisor.abs() < 1e-9 {
            return image::Rgb([0, 0, 0]);
        }
        let source_x = (a * u + b * v + p0[0]) / divisor;
        let source_y = (d * u + e * v + p0[1]) / divisor;
        if !(0.0..=max_x).contains(&source_x) || !(0.0..=max_y).contains(&source_y) {
            return image::Rgb([0, 0, 0]);
        }
        let x0 = source_x.floor() as u32;
        let y0 = source_y.floor() as u32;
        let x1 = (x0 + 1).min(width - 1);
        let y1 = (y0 + 1).min(height - 1);
        let tx = source_x - x0 as f64;
        let ty = source_y - y0 as f64;
        let p00 = source.get_pixel(x0, y0);
        let p10 = source.get_pixel(x1, y0);
        let p01 = source.get_pixel(x0, y1);
        let p11 = source.get_pixel(x1, y1);
        image::Rgb(std::array::from_fn(|channel| {
            let top = p00[channel] as f64 * (1.0 - tx) + p10[channel] as f64 * tx;
            let bottom = p01[channel] as f64 * (1.0 - tx) + p11[channel] as f64 * tx;
            (top * (1.0 - ty) + bottom * ty).round() as u8
        }))
    }))
}

fn crop_bounds_error(
    width: i64,
    height: i64,
    x: i64,
    y: i64,
    source_width: u32,
    source_height: u32,
) -> Option<String> {
    if width < 32 || height < 32 {
        return Some("Crop must be at least 32x32 pixels.".into());
    }
    if width > crate::calibration::MAX_DIMENSION as i64
        || height > crate::calibration::MAX_DIMENSION as i64
    {
        return Some("Crop dimensions exceed safe limits.".into());
    }
    if x >= source_width as i64 || y >= source_height as i64 || x + width <= 0 || y + height <= 0 {
        return Some("Crop does not overlap the image.".into());
    }
    None
}

enum CropShape<'a> {
    Rect {
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
    Quad(&'a [[f64; 2]]),
}

#[cfg(test)]
fn rect_crop(x: i64, y: i64, width: i64, height: i64) -> CropShape<'static> {
    CropShape::Rect {
        x,
        y,
        width,
        height,
    }
}

fn import_crop_to_store(
    store: &mut PresetStore,
    id: &str,
    path: &Path,
    angle: f64,
    crop_shape: CropShape<'_>,
) -> Result<(), String> {
    let preset = store
        .presets()
        .iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| "Preset not found.".to_string())?;
    let (target_width, target_height) = preset.calibration.dimensions()?;
    let source = rotate_expand(&processing::load_rgb(path)?, angle)?;
    let resized = match crop_shape {
        CropShape::Rect {
            x,
            y,
            width,
            height,
        } => {
            if width < 32 || height < 32 {
                return Err("Crop must be at least 32x32 pixels.".into());
            }
            let (source_width, source_height) = source.dimensions();
            if let Some(error) = crop_bounds_error(width, height, x, y, source_width, source_height)
            {
                return Err(error);
            }
            let crop = crop_with_padding(&source, x, y, width as u32, height as u32);
            // Triangle resampling matches the preview path's interpolation quality.
            image::imageops::resize(
                &crop,
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            )
        }
        CropShape::Quad(corners) => warp_quad(&source, corners, target_width, target_height)?,
    };
    let archive_dir = store.archive_dir();
    let store_root = archive_dir
        .parent()
        .ok_or_else(|| "Could not locate preset store directory.".to_string())?;
    let staging_dir = store_root.join("photos");
    std::fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    let staging = staging_dir.join(format!(
        ".crop-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if let Err(error) = resized.save(&staging) {
        let _ = std::fs::remove_file(&staging);
        return Err(error.to_string());
    }
    let imported = store
        .import_calibration_photo(id, &staging)
        .map_err(|error| error.to_string());
    let _ = std::fs::remove_file(staging);
    imported
}

fn stage_preset_crop(
    source_path: &Path,
    store_root: &Path,
    target_width: u32,
    target_height: u32,
    angle: f64,
    crop_shape: CropShape<'_>,
) -> Result<PathBuf, String> {
    let source = rotate_expand(&processing::load_rgb(source_path)?, angle)?;
    let resized = match crop_shape {
        CropShape::Rect {
            x,
            y,
            width,
            height,
        } => {
            let (source_width, source_height) = source.dimensions();
            if let Some(error) = crop_bounds_error(width, height, x, y, source_width, source_height)
            {
                return Err(error);
            }
            let crop = crop_with_padding(&source, x, y, width as u32, height as u32);
            image::imageops::resize(
                &crop,
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            )
        }
        CropShape::Quad(corners) => warp_quad(&source, corners, target_width, target_height)?,
    };
    let staging = store_root.join(format!(
        "crop-{}-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    ));
    resized.save(&staging).map_err(|error| error.to_string())?;
    Ok(staging)
}

#[tauri::command]
pub fn get_state(host: State<'_, Host>) -> UiState {
    build_state(&host.lock())
}

#[tauri::command]
pub fn add_laser(host: State<'_, Host>, name: String, watts: f64) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.add_laser(&name, watts) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Laser added".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn rename_laser(
    host: State<'_, Host>,
    id: String,
    name: String,
    watts: f64,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.rename_laser(&id, &name, watts) {
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
    inner.previews.clear();
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
    inner.previews.clear();
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
    inner.previews.clear();
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
    inner.previews.clear();
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
        speed: preset.speed,
        power_percent: preset.power_percent,
        dpi: preset.dpi,
        calibration_photo: existing.calibration_photo.clone(),
        scan_source_photo: existing.scan_source_photo.clone(),
        scan_crop: existing.scan_crop.clone(),
    };
    if let Err(error) = inner.store.update(updated) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.previews.clear();
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
    inner.previews.clear();
    inner.status = "Preset removed".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn import_scan(
    host: State<'_, Host>,
    id: String,
    path: String,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.import_calibration_photo(&id, Path::new(&path)) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    let _ = inner.store.keep_scan_source(&id, Path::new(&path));
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Calibration scan imported".to_string();
    Ok(host.publish(&mut inner))
}

fn crop_shape<'a>(
    corners: &'a Option<Vec<[f64; 2]>>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
) -> CropShape<'a> {
    match corners.as_deref() {
        Some(points) => CropShape::Quad(points),
        None => CropShape::Rect {
            x,
            y,
            width,
            height,
        },
    }
}

#[tauri::command]
pub async fn import_scan_crop(
    host: State<'_, Host>,
    id: String,
    path: String,
    angle: Option<f64>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    corners: Option<Vec<[f64; 2]>>,
) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    let result = import_crop_to_store(
        &mut inner.store,
        &id,
        Path::new(&path),
        angle.unwrap_or_default(),
        crop_shape(&corners, x, y, width, height),
    );
    if let Err(error) = result {
        return Err(host.fail(&mut inner, error));
    }
    let _ = inner.store.keep_scan_source(&id, Path::new(&path));
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Calibration scan imported".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn import_scan_crop_preset(
    host: State<'_, Host>,
    id: String,
    angle: Option<f64>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    corners: Option<Vec<[f64; 2]>>,
) -> Result<UiState, String> {
    let (source_path, store_root, target_width, target_height) = {
        let mut inner = host.lock();
        guard_busy(&inner)?;
        inner.error = None;
        let preset = match inner.store.presets().iter().find(|preset| preset.id == id) {
            Some(preset) => preset,
            None => return Err(host.fail(&mut inner, "Preset not found.".to_string())),
        };
        let source_path = match inner.store.scan_path(preset) {
            Some(path) => path,
            None => {
                return Err(host.fail(
                    &mut inner,
                    "This preset has no calibration scan.".to_string(),
                ));
            }
        };
        let (target_width, target_height) = match preset.calibration.dimensions() {
            Ok(dimensions) => dimensions,
            Err(error) => return Err(host.fail(&mut inner, error)),
        };
        let archive_dir = inner.store.archive_dir();
        let store_root = match archive_dir.parent() {
            Some(path) => path.to_path_buf(),
            None => {
                return Err(host.fail(
                    &mut inner,
                    "Could not locate preset store directory.".to_string(),
                ));
            }
        };
        (source_path, store_root, target_width, target_height)
    };
    let staging = match stage_preset_crop(
        &source_path,
        &store_root,
        target_width,
        target_height,
        angle.unwrap_or_default(),
        crop_shape(&corners, x, y, width, height),
    ) {
        Ok(path) => path,
        Err(error) => {
            let mut inner = host.lock();
            return Err(host.fail(&mut inner, error));
        }
    };
    let mut inner = host.lock();
    let imported = inner
        .store
        .import_calibration_photo(&id, &staging)
        .map_err(|error| error.to_string());
    let _ = std::fs::remove_file(staging);
    if let Err(error) = imported {
        return Err(host.fail(&mut inner, error));
    }
    let _ = inner
        .store
        .set_scan_crop(&id, Some(scan_crop(angle, x, y, width, height, &corners)));
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Calibration scan updated".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn import_scan_crop_source(
    host: State<'_, Host>,
    id: String,
    angle: Option<f64>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    corners: Option<Vec<[f64; 2]>>,
) -> Result<UiState, String> {
    let (source_path, store_root, target_width, target_height) = {
        let mut inner = host.lock();
        guard_busy(&inner)?;
        inner.error = None;
        let preset = match inner.store.presets().iter().find(|preset| preset.id == id) {
            Some(preset) => preset,
            None => return Err(host.fail(&mut inner, "Preset not found.".to_string())),
        };
        let source_path = match inner
            .store
            .source_path(preset)
            .or_else(|| inner.store.scan_path(preset))
        {
            Some(path) => path,
            None => {
                return Err(host.fail(
                    &mut inner,
                    "This preset has no calibration scan.".to_string(),
                ));
            }
        };
        let (target_width, target_height) = match preset.calibration.dimensions() {
            Ok(dimensions) => dimensions,
            Err(error) => return Err(host.fail(&mut inner, error)),
        };
        let store_root = match inner.store.archive_dir().parent() {
            Some(path) => path.to_path_buf(),
            None => {
                return Err(host.fail(
                    &mut inner,
                    "Could not locate preset store directory.".to_string(),
                ));
            }
        };
        (source_path, store_root, target_width, target_height)
    };
    let staging = match stage_preset_crop(
        &source_path,
        &store_root,
        target_width,
        target_height,
        angle.unwrap_or_default(),
        crop_shape(&corners, x, y, width, height),
    ) {
        Ok(path) => path,
        Err(error) => {
            let mut inner = host.lock();
            return Err(host.fail(&mut inner, error));
        }
    };
    let mut inner = host.lock();
    let imported = inner
        .store
        .import_calibration_photo(&id, &staging)
        .map_err(|error| error.to_string());
    let _ = std::fs::remove_file(staging);
    if let Err(error) = imported {
        return Err(host.fail(&mut inner, error));
    }
    let _ = inner
        .store
        .set_scan_crop(&id, Some(scan_crop(angle, x, y, width, height, &corners)));
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Calibration scan updated".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn remove_scan(host: State<'_, Host>, id: String) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    if let Err(error) = inner.store.detach_calibration_photo(&id) {
        return Err(host.fail(&mut inner, error.to_string()));
    }
    inner.cache.clear();
    inner.previews.clear();
    inner.status = "Calibration scan moved to the archive".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub async fn export_gauge(
    host: State<'_, Host>,
    id: String,
    path: String,
) -> Result<UiState, String> {
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
pub async fn preset_preview(host: State<'_, Host>, id: String) -> Result<PresetPreview, String> {
    // Resolve inputs and check the cache under the lock, then compute without it: a page
    // reload's get_state must never wait behind a gauge render.
    let (spec, scan_path) = {
        let inner = host.lock();
        if let Some(cached) = inner.previews.get(&id) {
            return Ok(cached.clone());
        }
        let preset = inner
            .store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .ok_or_else(|| "Preset not found.".to_string())?;
        (preset.calibration.clone(), inner.store.scan_path(preset))
    };
    let preview = build_preset_preview(&spec, scan_path)?;
    if let Ok(mut inner) = host.inner.lock() {
        inner.previews.insert(id, preview.clone());
    }
    Ok(preview)
}

#[tauri::command]
pub fn open_scan_archive(host: State<'_, Host>) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    let dir = inner.store.archive_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return Err(host.fail(
            &mut inner,
            format!("Could not create scan archive: {error}"),
        ));
    }
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(not(windows))]
    let opened = std::process::Command::new("xdg-open").arg(&dir).spawn();
    if let Err(error) = opened {
        return Err(host.fail(&mut inner, format!("Could not open scan archive: {error}")));
    }
    inner.status = "Scan archive opened".to_string();
    Ok(host.publish(&mut inner))
}

#[tauri::command]
pub fn clear_scan_archive(host: State<'_, Host>) -> Result<UiState, String> {
    let mut inner = host.lock();
    guard_busy(&inner)?;
    inner.error = None;
    match inner.store.clear_archive() {
        Ok(count) => {
            inner.status = format!("Cleared {count} archived scan(s)");
            Ok(host.publish(&mut inner))
        }
        Err(error) => Err(host.fail(&mut inner, error.to_string())),
    }
}

#[tauri::command]
pub async fn process_photo(
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
pub async fn save_engraving(
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
pub async fn save_simulation(
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
pub async fn save_report(
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
    let extensions: Vec<String> = extensions
        .iter()
        .map(|extension| extension.trim_start_matches('.').to_lowercase())
        .filter(|extension| !extension.is_empty())
        .collect();
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    // One combined filter: several filters would leave only the first one active on Windows.
    if extensions.len() == 1 {
        dialog = dialog.add_filter(extensions[0].to_uppercase(), &[extensions[0].as_str()]);
    } else if !extensions.is_empty() {
        let patterns: Vec<&str> = extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter("Images", &patterns);
    }
    dialog = dialog.add_filter("All files", &["*"]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::SampleRegion;

    fn temp_root() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("host-preview-{nanos}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn missing_scan_preview_serves_only_gauge() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Scanless", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let expected = preset.calibration.dimensions().unwrap();
        let id = preset.id.clone();
        store.add(preset).unwrap();
        let preset = store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .unwrap();
        let preview = build_preset_preview(&preset.calibration, store.scan_path(preset)).unwrap();
        assert!(preview.scan_data_url.is_none());
        assert!(preview.gauge_data_url.starts_with("data:image/png;base64,"));
        assert_eq!((preview.width, preview.height), expected);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_geometry_exposes_sampler_rectangles() {
        let spec = GridCalibrationSpecification::default();
        let geometry = build_scan_geometry(&spec).unwrap();
        let sample: SampleRegion = spec.sample_regions().unwrap()[0];
        assert_eq!(
            (geometry.width, geometry.height),
            spec.dimensions().unwrap()
        );
        assert_eq!(geometry.patches[0].level, sample.level);
        assert_eq!(geometry.patches[0].engraved, sample.engraved);
        assert_eq!(geometry.patches[0].left, sample.left);
        assert_eq!(geometry.patches[0].right, sample.right);
    }

    #[test]
    fn scan_source_preset_rejects_missing_scan() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Scanless", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        assert_eq!(
            preset_scan_path(&store, &id).unwrap_err(),
            "This preset has no calibration scan."
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_source_preset_loads_managed_scan() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Scanned", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let scan = root.join("scan.png");
        RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8, y as u8, 80]))
            .save(&scan)
            .unwrap();
        store.import_calibration_photo(&id, &scan).unwrap();
        let managed = preset_scan_path(&store, &id).unwrap();
        let result = scan_source_path(&managed).unwrap();
        assert_eq!((result.width, result.height), (64, 64));
        assert!(result.data_url.starts_with("data:image/png;base64,"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn original_scan_survives_crop_edits_and_legacy_scan_falls_back() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Original", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let picked = RgbImage::from_fn(200, 160, |x, y| {
            image::Rgb([x as u8, y as u8, (x + y) as u8])
        });
        let picked_path = root.join("picked.png");
        picked.save(&picked_path).unwrap();
        let picked_bytes = std::fs::read(&picked_path).unwrap();
        import_crop_to_store(
            &mut store,
            &id,
            &picked_path,
            0.0,
            rect_crop(40, 30, 64, 64),
        )
        .unwrap();
        store.keep_scan_source(&id, &picked_path).unwrap();
        let preset = store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .unwrap();
        let source_path = store.source_path(preset).unwrap();
        assert_eq!(std::fs::read(&source_path).unwrap(), picked_bytes);
        assert_ne!(
            processing::load_rgb(&source_path).unwrap().dimensions(),
            (64, 64)
        );
        let original = scan_source_path(&original_scan_path(&store, &id).unwrap()).unwrap();
        assert_eq!((original.width, original.height), (200, 160));

        let (target_width, target_height) = store.presets()[0].calibration.dimensions().unwrap();
        let managed = store.scan_path(&store.presets()[0]).unwrap();
        let archive_dir = store.archive_dir();
        let store_root = archive_dir.parent().unwrap().to_path_buf();
        let staging = stage_preset_crop(
            &managed,
            &store_root,
            target_width,
            target_height,
            0.0,
            rect_crop(0, 0, 64, 64),
        )
        .unwrap();
        store.import_calibration_photo(&id, &staging).unwrap();
        std::fs::remove_file(staging).unwrap();
        let preset = store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .unwrap();
        assert_eq!(
            std::fs::read(store.source_path(preset).unwrap()).unwrap(),
            picked_bytes
        );

        let mut legacy = Preset::new("Legacy scan", &laser_id);
        legacy.calibration.block_size = 32;
        legacy.calibration.rows = 2;
        legacy.calibration.columns = 2;
        let legacy_id = legacy.id.clone();
        store.add(legacy).unwrap();
        let gauge_scan = root.join("gauge.png");
        RgbImage::from_pixel(64, 64, image::Rgb([80, 80, 80]))
            .save(&gauge_scan)
            .unwrap();
        store
            .import_calibration_photo(&legacy_id, gauge_scan)
            .unwrap();
        let fallback = scan_source_path(&original_scan_path(&store, &legacy_id).unwrap()).unwrap();
        assert_eq!((fallback.width, fallback.height), (64, 64));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crop_import_resizes_and_rejections_preserve_current_scan() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Crop", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let gauge = store.presets()[0].calibration.generate().unwrap();
        let source = RgbImage::from_fn(200, 160, |x, y| {
            if (40..104).contains(&x) && (30..94).contains(&y) {
                let pixel = gauge.get_pixel(x - 40, y - 30)[0];
                image::Rgb([pixel, pixel, pixel])
            } else {
                image::Rgb([240, 240, 240])
            }
        });
        let source_path = root.join("large-source.png");
        source.save(&source_path).unwrap();
        import_crop_to_store(
            &mut store,
            &id,
            &source_path,
            0.0,
            rect_crop(40, 30, 64, 64),
        )
        .unwrap();
        let preset = store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .unwrap();
        let managed = store.scan_path(preset).unwrap();
        assert!(managed.exists());
        assert_eq!(
            processing::load_rgb(&managed).unwrap().dimensions(),
            (64, 64)
        );
        let initial_json = std::fs::read(root.join("presets.json")).unwrap();
        let initial_bytes = std::fs::read(&managed).unwrap();

        assert_eq!(
            import_crop_to_store(
                &mut store,
                &id,
                &source_path,
                0.0,
                rect_crop(400, 400, 32, 32)
            )
            .unwrap_err(),
            "Crop does not overlap the image."
        );
        assert_eq!(
            import_crop_to_store(&mut store, &id, &source_path, 0.0, rect_crop(0, 0, 16, 16))
                .unwrap_err(),
            "Crop must be at least 32x32 pixels."
        );
        assert_eq!(
            std::fs::read(root.join("presets.json")).unwrap(),
            initial_json
        );
        let preset = store
            .presets()
            .iter()
            .find(|preset| preset.id == id)
            .unwrap();
        assert_eq!(
            std::fs::read(store.scan_path(preset).unwrap()).unwrap(),
            initial_bytes
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn warp_quad_maps_an_upright_quad_one_to_one() {
        let source = RgbImage::from_pixel(64, 64, image::Rgb([200, 100, 50]));
        let corners = [[0.0, 0.0], [64.0, 0.0], [64.0, 64.0], [0.0, 64.0]];
        let warped = warp_quad(&source, &corners, 32, 32).unwrap();
        assert_eq!(warped.dimensions(), (32, 32));
        assert_eq!(warped.get_pixel(16, 16), &image::Rgb([200, 100, 50]));
    }

    #[test]
    fn warp_quad_fills_the_area_outside_the_photo_with_black() {
        let source = RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]));
        let corners = [
            [100.0, 100.0],
            [164.0, 100.0],
            [164.0, 164.0],
            [100.0, 164.0],
        ];
        let warped = warp_quad(&source, &corners, 16, 16).unwrap();
        assert_eq!(warped.get_pixel(8, 8), &image::Rgb([0, 0, 0]));
    }

    #[test]
    fn warp_quad_rejects_degenerate_or_non_finite_corners() {
        let source = RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]));
        let folded = [[10.0, 10.0], [10.0, 10.0], [10.0, 10.0], [10.0, 10.0]];
        assert!(warp_quad(&source, &folded, 16, 16).is_err());
        let not_finite = [[0.0, 0.0], [f64::NAN, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(warp_quad(&source, &not_finite, 16, 16).is_err());
    }

    #[test]
    fn crop_outside_pads_with_black() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Pad", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let source_path = root.join("small.png");
        RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]))
            .save(&source_path)
            .unwrap();
        import_crop_to_store(
            &mut store,
            &id,
            &source_path,
            0.0,
            rect_crop(48, 48, 32, 32),
        )
        .unwrap();
        let managed = store.scan_path(&store.presets()[0]).unwrap();
        let scan = processing::load_rgb(&managed).unwrap();
        assert_eq!(scan.dimensions(), (64, 64));
        assert_eq!(scan.get_pixel(4, 4), &image::Rgb([255, 255, 255]));
        assert_eq!(scan.get_pixel(60, 60), &image::Rgb([0, 0, 0]));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preset_crop_archives_previous_scan_and_rejects_out_of_bounds() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Crop", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let old_scan = RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8, y as u8, 90]));
        let source_path = root.join("initial.png");
        old_scan.save(&source_path).unwrap();
        store.import_calibration_photo(&id, &source_path).unwrap();
        let old_managed = {
            let preset = store
                .presets()
                .iter()
                .find(|preset| preset.id == id)
                .unwrap();
            store.scan_path(preset).unwrap()
        };
        let old_bytes = std::fs::read(&old_managed).unwrap();
        let (target_width, target_height) = store.presets()[0].calibration.dimensions().unwrap();
        let archive = store.archive_dir();
        let store_root = archive.parent().unwrap().to_path_buf();

        let staging = stage_preset_crop(
            &old_managed,
            &store_root,
            target_width,
            target_height,
            0.0,
            rect_crop(0, 0, 64, 64),
        )
        .unwrap();
        store.import_calibration_photo(&id, &staging).unwrap();
        std::fs::remove_file(staging).unwrap();
        let new_managed = {
            let preset = store
                .presets()
                .iter()
                .find(|preset| preset.id == id)
                .unwrap();
            store.scan_path(preset).unwrap()
        };
        assert_eq!(
            processing::load_rgb(&new_managed).unwrap().dimensions(),
            (64, 64)
        );
        assert!(!old_managed.exists());
        assert!(std::fs::read_dir(store.archive_dir())
            .unwrap()
            .any(|entry| {
                std::fs::read(entry.unwrap().path()).is_ok_and(|bytes| bytes == old_bytes)
            }));

        let current_bytes = std::fs::read(&new_managed).unwrap();
        let error = stage_preset_crop(
            &new_managed,
            &store_root,
            target_width,
            target_height,
            0.0,
            rect_crop(1000, 1000, 32, 32),
        )
        .unwrap_err();
        assert_eq!(error, "Crop does not overlap the image.");
        assert_eq!(std::fs::read(&new_managed).unwrap(), current_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_crop_preserves_original_and_rejects_out_of_bounds() {
        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Source crop", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();

        let gauge_scan = root.join("gauge.png");
        RgbImage::from_pixel(64, 64, image::Rgb([90, 90, 90]))
            .save(&gauge_scan)
            .unwrap();
        store.import_calibration_photo(&id, &gauge_scan).unwrap();
        let original_file = root.join("original.png");
        RgbImage::from_fn(160, 120, |x, y| {
            image::Rgb([x as u8, y as u8, (x.wrapping_add(y)) as u8])
        })
        .save(&original_file)
        .unwrap();
        store.keep_scan_source(&id, &original_file).unwrap();

        let original_path = store.source_path(&store.presets()[0]).unwrap();
        let original_bytes = std::fs::read(&original_path).unwrap();
        let (target_width, target_height) = store.presets()[0].calibration.dimensions().unwrap();
        let store_root = store.archive_dir().parent().unwrap().to_path_buf();
        let staging = stage_preset_crop(
            &original_path,
            &store_root,
            target_width,
            target_height,
            0.0,
            rect_crop(0, 0, 160, 120),
        )
        .unwrap();
        store.import_calibration_photo(&id, &staging).unwrap();
        std::fs::remove_file(staging).unwrap();
        assert_eq!(
            processing::load_rgb(&store.scan_path(&store.presets()[0]).unwrap())
                .unwrap()
                .dimensions(),
            (target_width, target_height)
        );
        assert_eq!(std::fs::read(&original_path).unwrap(), original_bytes);

        let managed_bytes = std::fs::read(&store.scan_path(&store.presets()[0]).unwrap()).unwrap();
        let error = stage_preset_crop(
            &original_path,
            &store_root,
            target_width,
            target_height,
            0.0,
            rect_crop(400, 400, 32, 32),
        )
        .unwrap_err();
        assert_eq!(error, "Crop does not overlap the image.");
        assert_eq!(
            std::fs::read(store.scan_path(&store.presets()[0]).unwrap()).unwrap(),
            managed_bytes
        );
        assert_eq!(std::fs::read(&original_path).unwrap(), original_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rotation_expands_canvas_and_rotated_crop_imports() {
        let square = RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8, y as u8, 120]));
        assert_eq!(rotate_expand(&square, 0.0).unwrap(), square);
        assert_eq!(rotate_expand(&square, 90.0).unwrap().dimensions(), (64, 64));
        let wide = RgbImage::from_pixel(128, 64, image::Rgb([170, 170, 170]));
        assert_eq!(rotate_expand(&wide, 90.0).unwrap().dimensions(), (64, 128));
        assert_eq!(rotate_expand(&square, 45.0).unwrap().dimensions(), (91, 91));

        let root = temp_root();
        let mut store = PresetStore::open(&root).unwrap();
        let laser_id = store.add_laser("Laser", 40.0).unwrap().id.clone();
        let mut preset = Preset::new("Rotated crop", &laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        let id = preset.id.clone();
        store.add(preset).unwrap();
        let source_path = root.join("source.png");
        square.save(&source_path).unwrap();
        let previous = root.join("previous.png");
        square.save(&previous).unwrap();
        store.import_calibration_photo(&id, previous).unwrap();
        let managed = store.scan_path(&store.presets()[0]).unwrap();
        let old_bytes = std::fs::read(&managed).unwrap();

        assert_eq!(
            import_crop_to_store(
                &mut store,
                &id,
                &source_path,
                200.0,
                rect_crop(0, 0, 64, 64)
            )
            .unwrap_err(),
            "rotation must be between -180 and 180 degrees"
        );
        assert_eq!(std::fs::read(&managed).unwrap(), old_bytes);
        import_crop_to_store(&mut store, &id, &source_path, 45.0, rect_crop(0, 0, 91, 91)).unwrap();
        assert_eq!(
            processing::load_rgb(&store.scan_path(&store.presets()[0]).unwrap())
                .unwrap()
                .dimensions(),
            (64, 64)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
