#[path = "../../../shared/host.rs"]
mod host;

use eframe::{CreationContext, Frame, egui};
use host::HostedEditor;
use lightcraft_engine::Session;
use lightcraft_ui_egui::{LightcraftApp, Services};

struct LightEditor(LightcraftApp);

impl HostedEditor for LightEditor {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self(LightcraftApp::new(Session::default(), Services::default()))
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.0.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.0.ui(ui);
    }
}

#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    host::run::<LightEditor>(app, "LightCraft", "LightCraft");
}
