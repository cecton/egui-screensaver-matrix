#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use egui_screensaver_matrix::MatrixBackground;

    #[derive(Default)]
    struct App {
        matrix: MatrixBackground,
    }

    impl eframe::App for App {
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            let ctx = ui.ctx().clone();
            self.matrix.paint(&ctx, frame.gl());
        }
    }

    eframe::run_native(
        "egui-screensaver-matrix demo",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
