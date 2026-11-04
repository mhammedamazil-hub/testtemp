#[path = "../../../shared/host.rs"]
mod host;

use eframe::{App, CreationContext, Frame, egui};
use effectcraft_engine::Session;
use effectcraft_ui_egui::EffectcraftApp;
use host::HostedEditor;

struct EffectEditor(EffectcraftApp);

impl HostedEditor for EffectEditor {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self(EffectcraftApp::new(Session::default()))
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
    host::run::<EffectEditor>(app, "EffectCraft", "EffectCraft");
}
