//! CraftOffice Sketch: drawing and sketching with layers, brushes and shapes. Part of CraftOffice
//! by AndyBcx; runs on Windows and Linux, and as WebAssembly in the browser and on Android.

pub mod app;
pub mod files;
pub mod i18n;
pub mod raster;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use app::SketchApp;
