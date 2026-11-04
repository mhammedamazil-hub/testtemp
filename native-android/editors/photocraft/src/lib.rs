#[path = "../../../shared/host.rs"]
mod host;

use eframe::{App, CreationContext, Frame, egui};
use host::HostedEditor;
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, Services};

struct PhotoEditor(PhotocraftApp);

impl HostedEditor for PhotoEditor {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self(PhotocraftApp::new(Session::new(), Services::default()))
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
    host::run::<PhotoEditor>(app, "PhotoCraft", "PhotoCraft");
}
