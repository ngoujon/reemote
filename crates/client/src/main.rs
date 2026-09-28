mod app;
mod net;
mod pin_store;
mod verifier;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Reemote",
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
