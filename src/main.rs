#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([820.0, 620.0])
            .with_title("Laser Engraving Optimizer"),
        ..Default::default()
    };
    eframe::run_native(
        "Laser Engraving Optimizer",
        options,
        Box::new(|cc| {
            Ok(Box::new(laser_engraving_optimizer::app::OptimizerApp::new(
                cc,
            )))
        }),
    )
}
