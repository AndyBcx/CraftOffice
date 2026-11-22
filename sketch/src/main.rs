#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    // A drawing or picture given on the command line (or by the CraftOffice launcher) opens.
    let open = std::env::args().nth(1).filter(|a| !a.starts_with("--")).and_then(|p| {
        let bytes = std::fs::read(&p).ok()?;
        let name = std::path::Path::new(&p).file_name()?.to_string_lossy().into_owned();
        Some((name, Some(p), bytes))
    });
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CraftOffice Sketch")
            .with_app_id("craftoffice-sketch")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([640.0, 420.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    #[cfg(windows)]
    {
        // As the other CraftOffice apps: DirectX 12 / Vulkan only (some OpenGL drivers crash).
        use eframe::egui_wgpu::{self, wgpu};
        let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        if wgpu::Backends::from_env().is_none() {
            setup.instance_descriptor.backends = wgpu::Backends::DX12 | wgpu::Backends::VULKAN;
        }
        options.wgpu_options.wgpu_setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    }
    #[cfg(not(windows))]
    {
        let _ = &mut options;
    }
    eframe::run_native("CraftOffice Sketch", options, Box::new(move |cc| Ok(Box::new(craftoffice_sketch::SketchApp::new(cc, open)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    craftoffice_sketch::web::start();
}
