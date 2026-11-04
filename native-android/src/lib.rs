#![allow(clippy::needless_pass_by_value)]

use std::time::Duration;

use eframe::{App, CreationContext, Frame, NativeOptions, egui};
use egui::{Align, Id, Layout, Order, RichText};
use log::LevelFilter;

#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("ARTCRAFT")
            .with_max_level(LevelFilter::Info),
    );

    let options = NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "ArtCraft Studio",
        options,
        Box::new(|cc| Ok(Box::new(ArtCraftApp::new(cc)))),
    ) {
        log::error!("Native ArtCraft app exited with an error: {error}");
    }
}

struct ArtCraftApp {
    editor: Option<Editor>,
    home: bool,
    first_frame_logged: bool,
}

impl ArtCraftApp {
    fn new(_cc: &CreationContext<'_>) -> Self {
        Self { editor: None, home: true, first_frame_logged: false }
    }

    fn open(&mut self, kind: EditorKind) {
        if self.editor.as_ref().is_none_or(|editor| editor.kind() != kind) {
            self.editor = Some(Editor::new(kind));
        }
        self.home = false;
        log::info!("Opening native Rust editor: {}", kind.title());
    }
}

impl App for ArtCraftApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        if !self.home {
            if let Some(editor) = &mut self.editor {
                editor.logic(ctx, frame);
            }
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !self.home {
            if let Some(editor) = &mut self.editor {
                editor.raw_input_hook(ctx, raw);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        // winit does not yet expose Android safe-area insets to egui, so reserve space for system bars.
        egui::Panel::top("android_status_bar_space").show(ui, |ui| ui.set_height(32.0));
        egui::Panel::bottom("android_navigation_bar_space").show(ui, |ui| ui.set_height(24.0));
        let ctx = ui.ctx().clone();
        if self.home {
            let mut selected = None;
            egui::CentralPanel::default().show(&ctx, |ui| {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("ARTCRAFT STUDIO").size(13.0).strong().color(egui::Color32::from_rgb(126, 190, 255)));
                    ui.add_space(5.0);
                    ui.heading(RichText::new("Choose your workspace").size(27.0));
                    ui.label("Native Rust editors · Android platform integration in progress");
                });
                ui.add_space(24.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for kind in EditorKind::ALL {
                        let width = ui.available_width();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_min_width(width);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.heading(kind.title());
                                    ui.label(kind.description());
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Open").clicked() {
                                        selected = Some(kind);
                                    }
                                });
                            });
                        });
                        ui.add_space(8.0);
                    }
                });
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(RichText::new("Runs locally · Rust engine and native egui interface").small().weak());
                });
            });
            if let Some(kind) = selected {
                self.open(kind);
            }
        } else if let Some(editor) = &mut self.editor {
            editor.ui(ui, frame);
            let mut go_home = false;
            egui::Area::new(Id::new("artcraft_native_navigation"))
                .order(Order::Foreground)
                .fixed_pos(egui::pos2(10.0, 36.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("‹ Studio").clicked() {
                                go_home = true;
                            }
                            ui.label(RichText::new(editor.kind().title()).strong());
                        });
                    });
                });
            if go_home {
                self.home = true;
            }
        } else {
            self.home = true;
        }

        if !self.first_frame_logged {
            self.first_frame_logged = true;
            log::info!("ARTCRAFT_NATIVE_FIRST_FRAME");
        }
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorKind {
    Photo,
    Vector,
    Film,
    Light,
    Pdf,
    Effect,
    Design,
}

impl EditorKind {
    const ALL: [Self; 7] = [Self::Photo, Self::Vector, Self::Film, Self::Light, Self::Pdf, Self::Effect, Self::Design];

    fn title(self) -> &'static str {
        match self {
            Self::Photo => "PhotoCraft",
            Self::Vector => "VectorCraft",
            Self::Film => "FilmCraft",
            Self::Light => "LightCraft",
            Self::Pdf => "PdfCraft",
            Self::Effect => "EffectCraft",
            Self::Design => "DesignCraft",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Photo => "Raster painting, layers, masks and image processing.",
            Self::Vector => "Vector illustration, paths, shapes and SVG.",
            Self::Film => "Multitrack video, audio and timeline editing.",
            Self::Light => "Photo library, RAW development and color.",
            Self::Pdf => "PDF viewing, annotation and page operations.",
            Self::Effect => "Motion graphics, compositing and effects.",
            Self::Design => "Page layout, typography and publishing.",
        }
    }
}

enum Editor {
    Photo(photocraft_ui_egui::PhotocraftApp),
    Vector(vectorcraft_ui_egui::VectorcraftApp),
    Film(filmcraft_ui_egui::FilmcraftApp),
    Light(lightcraft_ui_egui::LightcraftApp),
    Pdf(pdfcraft_ui_egui::PdfCraftApp),
    Effect(effectcraft_ui_egui::EffectcraftApp),
    Design(designcraft_ui_egui::DesignApp),
}

impl Editor {
    fn new(kind: EditorKind) -> Self {
        match kind {
            EditorKind::Photo => Self::Photo(photocraft_ui_egui::PhotocraftApp::new(
                photocraft_engine::Session::new(),
                photocraft_ui_egui::Services::default(),
            )),
            EditorKind::Vector => Self::Vector(vectorcraft_ui_egui::VectorcraftApp::new(
                vectorcraft_engine::Session::default(),
                vectorcraft_ui_egui::Services::default(),
            )),
            EditorKind::Film => Self::Film(filmcraft_ui_egui::FilmcraftApp::new(filmcraft_engine::Session::default())),
            EditorKind::Light => Self::Light(lightcraft_ui_egui::LightcraftApp::new(
                lightcraft_engine::Session::default(),
                lightcraft_ui_egui::Services::default(),
            )),
            EditorKind::Pdf => Self::Pdf(pdfcraft_ui_egui::PdfCraftApp::new()),
            EditorKind::Effect => Self::Effect(effectcraft_ui_egui::EffectcraftApp::new(effectcraft_engine::Session::default())),
            EditorKind::Design => Self::Design(designcraft_ui_egui::DesignApp::new(
                designcraft_engine::Session::new(),
                designcraft_ui_egui::Services::default(),
            )),
        }
    }

    fn kind(&self) -> EditorKind {
        match self {
            Self::Photo(_) => EditorKind::Photo,
            Self::Vector(_) => EditorKind::Vector,
            Self::Film(_) => EditorKind::Film,
            Self::Light(_) => EditorKind::Light,
            Self::Pdf(_) => EditorKind::Pdf,
            Self::Effect(_) => EditorKind::Effect,
            Self::Design(_) => EditorKind::Design,
        }
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) {
        match self {
            Self::Photo(app) => App::logic(app, ctx, frame),
            Self::Vector(app) => {
                app.begin_host_iteration();
                app.logic(ctx);
            }
            Self::Film(app) => App::logic(app, ctx, frame),
            Self::Light(app) => app.logic(ctx),
            Self::Pdf(app) => App::logic(app, ctx, frame),
            Self::Effect(app) => App::logic(app, ctx, frame),
            Self::Design(app) => app.logic(ctx),
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        match self {
            Self::Photo(app) => App::raw_input_hook(app, ctx, raw),
            Self::Vector(_) => {}
            Self::Film(app) => App::raw_input_hook(app, ctx, raw),
            Self::Light(app) => app.raw_input_hook(ctx, raw),
            Self::Pdf(app) => App::raw_input_hook(app, ctx, raw),
            Self::Effect(app) => App::raw_input_hook(app, ctx, raw),
            Self::Design(app) => app.raw_input_hook(ctx, raw),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        match self {
            Self::Photo(app) => App::ui(app, ui, frame),
            Self::Vector(app) => app.ui(ui),
            Self::Film(app) => App::ui(app, ui, frame),
            Self::Light(app) => app.ui(ui),
            Self::Pdf(app) => App::ui(app, ui, frame),
            Self::Effect(app) => App::ui(app, ui, frame),
            Self::Design(app) => app.ui(ui),
        }
    }
}
