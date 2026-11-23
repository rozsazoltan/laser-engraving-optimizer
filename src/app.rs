use crate::{
    calibration::{Calibration, GridCalibrationSpecification},
    presets::{Preset, PresetStore},
    processing::{load_rgb, process_photo, ColorMethod, ProcessingSettings},
};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::{GrayImage, RgbImage};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};

const PREVIEW_EDGE: u32 = 640;

enum JobResult {
    InputLoaded(PathBuf, Result<RgbImage, String>),
    Imported(PresetStore, Result<(), String>),
    Processed(Result<(GrayImage, GrayImage), String>),
    Saved(Result<(), String>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CloseIntent {
    #[default]
    None,
    Confirm,
    SaveAfterPreset,
    CloseNow,
}

impl CloseIntent {
    fn request(&mut self) {
        *self = Self::Confirm;
    }

    fn cancel(&mut self) {
        *self = Self::None;
    }

    fn revert(&mut self) {
        *self = Self::None;
    }

    fn save(&mut self) {
        if *self == Self::Confirm {
            *self = Self::SaveAfterPreset;
        }
    }

    fn saved(&mut self) {
        if *self == Self::SaveAfterPreset {
            *self = Self::CloseNow;
        }
    }

    fn take_close(&mut self) -> bool {
        if *self == Self::CloseNow {
            *self = Self::None;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Draft {
    name: String,
    note: String,
    calibration: GridCalibrationSpecification,
    processing: ProcessingSettings,
}

impl From<&Preset> for Draft {
    fn from(preset: &Preset) -> Self {
        Self {
            name: preset.name.clone(),
            note: preset.note.clone(),
            calibration: preset.calibration.clone(),
            processing: preset.processing.clone(),
        }
    }
}

impl Draft {
    fn apply(&self, preset: &mut Preset) {
        preset.name.clone_from(&self.name);
        preset.note.clone_from(&self.note);
        preset.calibration = self.calibration.clone();
        preset.processing = self.processing.clone();
    }
}

pub struct OptimizerApp {
    store: Option<PresetStore>,
    draft: Option<Draft>,
    selected_id: Option<String>,
    input_path: Option<PathBuf>,
    input_preview: Option<TextureHandle>,
    result_preview: Option<TextureHandle>,
    result: Option<GrayImage>,
    job: Option<Receiver<JobResult>>,
    status: String,
    error: Option<String>,
    close_intent: CloseIntent,
}

impl OptimizerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            store: None,
            draft: None,
            selected_id: None,
            input_path: None,
            input_preview: None,
            result_preview: None,
            result: None,
            job: None,
            status: String::new(),
            error: None,
            close_intent: CloseIntent::None,
        };
        match PresetStore::open_default() {
            Ok(mut store) => {
                if store.presets().is_empty() {
                    let preset = Preset::new("My preset");
                    if let Err(error) = store.add(preset) {
                        app.error = Some(error.to_string());
                    }
                }
                if let Some(preset) = store
                    .selected()
                    .or_else(|| store.presets().first())
                    .cloned()
                {
                    app.selected_id = Some(preset.id.clone());
                    app.draft = Some(Draft::from(&preset));
                    if store.selected().is_none() {
                        if let Err(error) = store.select(&preset.id) {
                            app.error = Some(error.to_string());
                        }
                    }
                }
                app.store = Some(store);
            }
            Err(error) => app.error = Some(error.to_string()),
        }
        app
    }

    fn dirty(&self) -> bool {
        let Some((store, draft, id)) = self
            .store
            .as_ref()
            .zip(self.draft.as_ref())
            .zip(self.selected_id.as_ref())
            .map(|((s, d), i)| (s, d, i))
        else {
            return false;
        };
        store
            .presets()
            .iter()
            .find(|preset| &preset.id == id)
            .is_some_and(|preset| &Draft::from(preset) != draft)
    }

    fn clear_result(&mut self) {
        self.result = None;
        self.result_preview = None;
    }

    fn set_error(&mut self, error: impl ToString) {
        self.error = Some(error.to_string());
    }

    fn active_preset(&self) -> Option<&Preset> {
        self.store
            .as_ref()?
            .presets()
            .iter()
            .find(|p| Some(&p.id) == self.selected_id.as_ref())
    }

    fn start_job(&mut self, work: impl FnOnce() -> JobResult + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        self.status = "Working…".into();
        self.error = None;
    }

    fn import_scan(&mut self, source: PathBuf) {
        let Some(mut store) = self.store.take() else {
            return;
        };
        let Some(id) = self.selected_id.clone() else {
            self.store = Some(store);
            return;
        };
        self.clear_result();
        self.start_job(move || {
            let result = store
                .import_calibration_photo(&id, source)
                .map_err(|e| e.to_string());
            JobResult::Imported(store, result)
        });
    }

    fn process(&mut self) {
        let (Some(path), Some(preset), Some(store)) = (
            self.input_path.clone(),
            self.active_preset().cloned(),
            self.store.as_ref(),
        ) else {
            return;
        };
        let Some(scan_path) = store.calibration_photo_path(&preset) else {
            self.set_error("Choose and save calibration scan before processing.");
            return;
        };
        self.clear_result();
        self.start_job(move || {
            let result = (|| {
                let spec = &preset.calibration;
                let scan = load_rgb(scan_path)?;
                let calibration = Calibration::from_scan(spec, &scan)?;
                let photo = load_rgb(path)?;
                let processed = process_photo(&photo, &calibration, &preset.processing)?;
                let preview = thumbnail_gray(&processed.engraving);
                Ok((processed.engraving, preview))
            })();
            JobResult::Processed(result)
        });
    }

    fn make_gauge(&mut self) {
        let Some(draft) = self.draft.clone() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("calibration-gauge.png")
            .add_filter("PNG image", &["png"])
            .save_file()
        else {
            return;
        };
        self.start_job(move || {
            let result = draft.calibration.generate().and_then(|image| {
                image
                    .save_with_format(path, image::ImageFormat::Png)
                    .map_err(|error| format!("Could not save calibration PNG: {error}"))
            });
            JobResult::Saved(result)
        });
    }

    fn save_draft(&mut self) -> bool {
        let (Some(mut preset), Some(draft)) = (self.active_preset().cloned(), self.draft.clone())
        else {
            return false;
        };
        draft.apply(&mut preset);
        let Some(store) = self.store.as_mut() else {
            return false;
        };
        match store.update(preset) {
            Ok(()) => {
                self.status = "Preset saved.".into();
                self.error = None;
                self.close_intent.saved();
                true
            }
            Err(error) => {
                self.set_error(error);
                false
            }
        }
    }

    fn save_output(&mut self, path: PathBuf) {
        let Some(image) = self.result.clone() else {
            return;
        };
        self.start_job(move || {
            JobResult::Saved(
                image
                    .save_with_format(path, image::ImageFormat::Png)
                    .map_err(|e| format!("Could not save PNG: {e}")),
            )
        });
    }

    fn close_when_safe(&mut self, ctx: &egui::Context) {
        if self.close_intent.take_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn revert_draft(&mut self) {
        if let Some(preset) = self.active_preset() {
            self.draft = Some(Draft::from(preset));
        }
        self.clear_result();
        self.close_intent.revert();
    }

    fn save_and_exit(&mut self) {
        self.close_intent.save();
        if !self.save_draft() {
            self.close_intent = CloseIntent::Confirm;
        }
    }

    fn discard_and_exit(&mut self) {
        self.revert_draft();
        self.close_intent = CloseIntent::CloseNow;
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.job.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(JobResult::Imported(store, result)) => {
                self.store = Some(store);
                self.job = None;
                match result {
                    Ok(()) => self.status = "Calibration photo saved to this preset.".into(),
                    Err(error) => self.set_error(error),
                }
            }
            Ok(JobResult::Processed(result)) => {
                self.job = None;
                match result {
                    Ok((image, preview)) => {
                        let dimensions = image.dimensions();
                        self.result_preview = Some(gray_texture(ctx, &preview, "result"));
                        self.result = Some(image);
                        self.status = format!("Ready · {} × {} px", dimensions.0, dimensions.1);
                    }
                    Err(error) => self.set_error(error),
                }
            }
            Ok(JobResult::Saved(result)) => {
                self.job = None;
                match result {
                    Ok(()) => self.status = "PNG saved.".into(),
                    Err(error) => self.set_error(error),
                }
            }
            Ok(JobResult::InputLoaded(path, result)) => {
                self.job = None;
                match result {
                    Ok(image) => {
                        self.input_preview = Some(rgb_texture(ctx, &image, "input"));
                        self.input_path = Some(path);
                        self.clear_result();
                        self.status = "Photo ready.".into();
                    }
                    Err(error) => self.set_error(error),
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                self.set_error("Background task stopped unexpectedly.");
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }

    fn show_preview(ui: &mut egui::Ui, texture: Option<&TextureHandle>, empty: &str) {
        if let Some(texture) = texture {
            let size = fit_preview_size(texture.size_vec2(), ui.available_size());
            ui.add(egui::Image::new(texture).fit_to_exact_size(size));
        } else {
            ui.centered_and_justified(|ui| ui.label(empty));
        }
    }

    fn controls_enabled(&self) -> bool {
        self.job.is_none() && self.store.is_some() && !self.dirty()
    }
}

impl eframe::App for OptimizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_job(&ctx);
        self.close_when_safe(&ctx);
        let busy = self.job.is_some();
        let dirty = self.dirty();
        if ui.ctx().input(|i| i.viewport().close_requested()) && (dirty || busy) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_intent.request();
        }
        ui.horizontal(|ui| {
            ui.heading("Engraving optimizer");
            ui.add_space(12.0);
            ui.label("LOCAL · CALIBRATED GRAYSCALE");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if busy {
                    ui.spinner();
                    ui.label("Working");
                }
                if dirty {
                    ui.label("Unsaved changes");
                }
            });
        });
        ui.separator();
        if self.close_intent != CloseIntent::None {
            ui.horizontal(|ui| {
                if busy {
                    ui.label("Wait for current task to finish before closing.");
                } else if dirty {
                    ui.label("Save or discard unsaved preset changes before closing.");
                    if ui.button("Save & exit").clicked() {
                        self.save_and_exit();
                    }
                    if ui.button("Discard & exit").clicked() {
                        self.discard_and_exit();
                    }
                } else {
                    ui.label("Close the app?");
                    if ui.button("Close app").clicked() {
                        if self.dirty() {
                            self.close_intent.request();
                        } else {
                            self.close_intent = CloseIntent::CloseNow;
                        }
                    }
                }
                if ui.button("Keep editing").clicked() {
                    self.close_intent.cancel();
                }
            });
            ui.separator();
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(170, 45, 40), error);
                    if ui.small_button("Dismiss").clicked() { self.error = None; }
                });
                ui.add_space(8.0);
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            let Some(store) = self.store.as_ref() else {
                if self.job.is_some() {
                    ui.label("Saving calibration photo…");
                } else {
                    ui.heading("Preset storage unavailable");
                    ui.label("Your existing preset file was not changed. Fix the reported error and restart.");
                }
                return;
            };
            let presets = store.presets().to_vec();
            ui.horizontal(|ui| {
                ui.label("Preset");
                let current_name = self.active_preset().map_or("Select preset", |p| p.name.as_str()).to_owned();
                ui.add_enabled_ui(can_switch_preset(busy), |ui| {
                    egui::ComboBox::from_id_salt("presets").selected_text(current_name).show_ui(ui, |ui| {
                        for preset in &presets {
                            let selected = self.selected_id.as_deref() == Some(&preset.id);
                            if ui.selectable_label(selected, &preset.name).clicked() {
                                if dirty {
                                    self.status = "Save or revert changes before switching presets.".into();
                                } else if let Some(store) = self.store.as_mut() {
                                    match store.select(&preset.id) {
                                        Ok(()) => {
                                            self.selected_id = Some(preset.id.clone());
                                            self.draft = Some(Draft::from(preset));
                                            self.clear_result();
                                        }
                                        Err(error) => self.set_error(error),
                                    }
                                }
                            }
                        }
                    });
                });
                if ui.add_enabled(!busy && !dirty, egui::Button::new("＋ New preset")).clicked() {
                    if let Some(store) = self.store.as_mut() {
                        let preset = Preset::new(format!("Preset {}", store.presets().len() + 1));
                        match store.add(preset.clone()).and_then(|_| store.select(&preset.id)) {
                            Ok(()) => {
                                self.selected_id = Some(preset.id.clone());
                                self.draft = Some(Draft::from(&preset));
                                self.clear_result();
                            }
                            Err(error) => self.set_error(error),
                        }
                    }
                }
                if ui.add_enabled(dirty && !busy, egui::Button::new("Save preset")).clicked() {
                    self.save_draft();
                }
                if ui.add_enabled(dirty && !busy, egui::Button::new("Revert")).clicked() {
                    self.revert_draft();
                }
            });
            ui.separator();
            ui.columns(2, |columns| {
                columns[0].heading("Image");
                columns[0].label("Choose a photo, then process with selected calibration.");
                if columns[0].add_enabled(self.controls_enabled(), egui::Button::new("Choose image…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg"]).pick_file()
                    {
                        let worker_path = path.clone();
                        self.start_job(move || {
                            JobResult::InputLoaded(
                                path,
                                load_rgb(worker_path).map(|image| thumbnail_rgb(&image)),
                            )
                        });
                    }
                }
                columns[0].add_space(8.0);
                egui::Frame::group(columns[0].style()).show(&mut columns[0], |ui| {
                    ui.set_min_height(245.0);
                    Self::show_preview(ui, self.input_preview.as_ref(), "No image selected");
                });
                columns[0].add_space(10.0);
                columns[0].heading("Calibration");
                columns[0].label(format!(
                    "Scan required: {} × {} px. Crop, align and resize to gauge dimensions.",
                    self.draft.as_ref().map_or(0, |d| d.calibration.columns * d.calibration.block_size),
                    self.draft.as_ref().map_or(0, |d| d.calibration.rows * d.calibration.block_size),
                ));
                let scan_ready = self.active_preset().and_then(|p| self.store.as_ref()?.calibration_photo_path(p)).is_some();
                columns[0].label(if scan_ready { "Calibration scan saved with preset." } else { "No calibration scan saved." });
                if columns[0].add_enabled(!busy && !dirty, egui::Button::new("Choose calibration scan…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg"]).pick_file()
                    { self.import_scan(path); }
                }
                if columns[0].add_enabled(!busy, egui::Button::new("Save calibration gauge PNG…")).clicked() {
                    self.make_gauge();
                }

                columns[1].heading("Calibration preset");
                columns[1].label("Calibration scan, grid and processing settings belong to this preset.");
                let enabled = !busy && self.draft.is_some();
                let draft_before_controls = self.draft.clone();
                if let Some(draft) = self.draft.as_mut() {
                    egui::Grid::new("preset-fields").num_columns(2).spacing([10.0, 8.0]).show(&mut columns[1], |ui| {
                        ui.label("Name"); ui.add_enabled(enabled, egui::TextEdit::singleline(&mut draft.name)); ui.end_row();
                        ui.label("Note"); ui.add_enabled(enabled, egui::TextEdit::singleline(&mut draft.note)); ui.end_row();
                        ui.label("Block size"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.calibration.block_size).range(32..=4096)); ui.end_row();
                        ui.label("Rows"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.calibration.rows).range(2..=64)); ui.end_row();
                        ui.label("Columns"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.calibration.columns).range(2..=64)); ui.end_row();
                        ui.label("Curve exponent"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.calibration.exponent).range(0.1..=8.0).speed(0.1)); ui.end_row();
                        ui.label("Gamma"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.processing.gamma).range(0.1..=4.0).speed(0.05)); ui.end_row();
                        ui.label("CLAHE"); ui.add_enabled(enabled, egui::DragValue::new(&mut draft.processing.clahe_clip).range(0.0..=16.0).speed(0.1)); ui.end_row();
                        ui.label("Color"); ui.add_enabled_ui(enabled, |ui| {
                            egui::ComboBox::from_id_salt("color-method").selected_text(match draft.processing.color_method {
                                ColorMethod::Decolorize => "Decolorize", ColorMethod::Luma => "Luma",
                            }).show_ui(ui, |ui| {
                                ui.selectable_value(&mut draft.processing.color_method, ColorMethod::Decolorize, "Decolorize");
                                ui.selectable_value(&mut draft.processing.color_method, ColorMethod::Luma, "Luma");
                            });
                        }); ui.end_row();
                    });
                }
                if draft_before_controls != self.draft {
                    self.clear_result();
                }
                columns[1].add_space(14.0);
                let can_process = self.controls_enabled() && self.input_path.is_some() && scan_ready;
                if columns[1].add_enabled(can_process, egui::Button::new("Process photo")).clicked() { self.process(); }
                if let Some((width, height)) = self.result.as_ref().map(GrayImage::dimensions) {
                    if columns[1].add_enabled(!busy, egui::Button::new("Save engraving PNG…")).clicked() {
                        if let Some(path) = rfd::FileDialog::new().set_file_name("engraving.png")
                            .add_filter("PNG image", &["png"]).save_file()
                        { self.save_output(path); }
                    }
                    columns[1].add_space(8.0);
                    columns[1].heading(format!("Engraving · {width} × {height} px"));
                    egui::Frame::group(columns[1].style()).show(&mut columns[1], |ui| {
                        ui.set_min_height(245.0);
                        Self::show_preview(ui, self.result_preview.as_ref(), "Process a photo to preview");
                    });
                } else {
                    columns[1].add_space(8.0);
                    columns[1].heading("Engraving preview");
                    egui::Frame::group(columns[1].style()).show(&mut columns[1], |ui| {
                        ui.set_min_height(245.0);
                        Self::show_preview(ui, None, "Your full-resolution result appears here");
                    });
                }
            });
        });
    }
}

fn thumbnail_dimensions(width: u32, height: u32) -> (u32, u32) {
    let scale = (PREVIEW_EDGE as f64 / width.max(height) as f64).min(1.0);
    (
        (width as f64 * scale).round().max(1.0) as u32,
        (height as f64 * scale).round().max(1.0) as u32,
    )
}

fn can_switch_preset(busy: bool) -> bool {
    !busy
}

fn fit_preview_size(texture: egui::Vec2, available: egui::Vec2) -> egui::Vec2 {
    if texture.x <= 0.0 || texture.y <= 0.0 || available.x <= 0.0 || available.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    texture
        * (available.x / texture.x)
            .min(available.y / texture.y)
            .min(1.0)
}

fn thumbnail_rgb(image: &RgbImage) -> RgbImage {
    let (width, height) = thumbnail_dimensions(image.width(), image.height());
    image::imageops::thumbnail(image, width, height)
}

fn thumbnail_gray(image: &GrayImage) -> GrayImage {
    let (width, height) = thumbnail_dimensions(image.width(), image.height());
    image::imageops::thumbnail(image, width, height)
}

fn rgb_texture(ctx: &egui::Context, image: &RgbImage, name: &str) -> TextureHandle {
    let (w, h) = image.dimensions();
    let pixels = image
        .pixels()
        .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
        .collect();
    ctx.load_texture(
        name,
        ColorImage::new([w as usize, h as usize], pixels),
        TextureOptions::LINEAR,
    )
}

fn gray_texture(ctx: &egui::Context, image: &GrayImage, name: &str) -> TextureHandle {
    let (w, h) = image.dimensions();
    let pixels = image
        .pixels()
        .map(|p| egui::Color32::from_gray(p[0]))
        .collect();
    ctx.load_texture(
        name,
        ColorImage::new([w as usize, h as usize], pixels),
        TextureOptions::LINEAR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_bounds_large_and_preserves_small_images() {
        assert_eq!(thumbnail_dimensions(4000, 2000), (640, 320));
        assert_eq!(thumbnail_dimensions(320, 200), (320, 200));
    }

    #[test]
    fn preview_fits_without_distorting_aspect_ratio() {
        let landscape = fit_preview_size(egui::vec2(400.0, 200.0), egui::vec2(100.0, 100.0));
        assert_eq!(landscape, egui::vec2(100.0, 50.0));
        let portrait = fit_preview_size(egui::vec2(200.0, 400.0), egui::vec2(100.0, 100.0));
        assert_eq!(portrait, egui::vec2(50.0, 100.0));
    }

    #[test]
    fn preset_switch_is_disabled_while_a_job_runs() {
        assert!(can_switch_preset(false));
        assert!(!can_switch_preset(true));
    }

    #[test]
    fn close_intent_can_cancel_revert_and_close_only_after_save() {
        let mut intent = CloseIntent::Confirm;
        intent.cancel();
        assert_eq!(intent, CloseIntent::None);

        intent.request();
        intent.revert();
        assert_eq!(intent, CloseIntent::None);

        intent.request();
        intent.save();
        assert_eq!(intent, CloseIntent::SaveAfterPreset);
        intent.saved();
        assert_eq!(intent, CloseIntent::CloseNow);
        assert!(intent.take_close());
        assert_eq!(intent, CloseIntent::None);
    }

    #[test]
    fn draft_tracks_all_persisted_preset_fields() {
        let preset = Preset::new("Fixture");
        let mut draft = Draft::from(&preset);
        assert_eq!(draft, Draft::from(&preset));
        draft.calibration.rows += 1;
        assert_ne!(draft, Draft::from(&preset));
        draft = Draft::from(&preset);
        draft.processing.gamma += 0.1;
        assert_ne!(draft, Draft::from(&preset));
    }
}
