#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use laser_engraving_optimizer::host;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            host::init(app).map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host::get_state,
            host::add_laser,
            host::rename_laser,
            host::remove_laser,
            host::select_laser,
            host::select_preset,
            host::add_preset,
            host::save_preset,
            host::remove_preset,
            host::import_scan,
            host::remove_scan,
            host::export_gauge,
            host::preset_preview,
            host::open_scan_archive,
            host::clear_scan_archive,
            host::scan_geometry,
            host::scan_source,
            host::scan_source_preset,
            host::scan_source_original,
            host::import_scan_crop,
            host::import_scan_crop_preset,
            host::import_scan_crop_source,
            host::process_photo,
            host::save_engraving,
            host::save_simulation,
            host::save_report,
            host::pick_open_file,
            host::pick_save_file,
            host::minimize_window,
            host::toggle_maximize_window,
            host::close_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running engraving");
}
