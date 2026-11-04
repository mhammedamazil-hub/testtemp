#[path = "../../../shared/host.rs"]
mod host;

use eframe::{App, CreationContext, Frame, egui};
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use host::HostedEditor;

struct FilmEditor(FilmcraftApp);

impl HostedEditor for FilmEditor {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self(FilmcraftApp::new(Session::default()))
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        App::logic(&mut self.0, ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        App::raw_input_hook(&mut self.0, ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        App::ui(&mut self.0, ui, frame);
    }
}

#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    host::run::<FilmEditor>(app, "FilmCraft", "FilmCraft");
}
