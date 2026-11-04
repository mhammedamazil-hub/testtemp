use std::time::Duration;

use eframe::{App, CreationContext, Frame, NativeOptions, egui};
use log::LevelFilter;

pub trait HostedEditor: 'static {
    fn new(cc: &CreationContext<'_>) -> Self;

    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut Frame) {}

    fn raw_input_hook(&mut self, _ctx: &egui::Context, _raw: &mut egui::RawInput) {}

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame);
}

struct NativeHost<E> {
    editor: E,
    editor_name: &'static str,
    first_frame_logged: bool,
}

impl<E: HostedEditor> App for NativeHost<E> {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        self.editor.logic(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.editor.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        // winit has not yet exposed Android safe-area insets to egui; reserve system-bar space.
        egui::Panel::top("android_status_bar_space").show(ui, |ui| ui.set_height(32.0));
        egui::Panel::bottom("android_navigation_bar_space").show(ui, |ui| ui.set_height(24.0));
        self.editor.ui(ui, frame);

        if !self.first_frame_logged {
            self.first_frame_logged = true;
            log::info!("ARTCRAFT_NATIVE_FIRST_FRAME:{}", self.editor_name);
        }
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}

pub fn run<E: HostedEditor>(
    android_app: winit::platform::android::activity::AndroidApp,
    editor_name: &'static str,
    window_title: &'static str,
) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("ARTCRAFT")
            .with_max_level(LevelFilter::Info),
    );

    let options = NativeOptions {
        android_app: Some(android_app),
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        window_title,
        options,
        Box::new(move |cc| {
            Ok(Box::new(NativeHost {
                editor: E::new(cc),
                editor_name,
                first_frame_logged: false,
            }))
        }),
    ) {
        log::error!("{window_title} native editor exited with an error: {error}");
    }
}
