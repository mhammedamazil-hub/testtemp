#[path = "../../../shared/host.rs"]
mod host;

use eframe::{CreationContext, Frame, egui};
use host::HostedEditor;
use vectorcraft_engine::Session;
use vectorcraft_ui_egui::{Services, VectorcraftApp};

struct VectorEditor(VectorcraftApp);

impl HostedEditor for VectorEditor {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self(VectorcraftApp::new(Session::default(), Services::default()))
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.0.begin_host_iteration();
        self.0.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.0.ui(ui);
    }
}

#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    host::run::<VectorEditor>(app, "VectorCraft", "VectorCraft");
}
