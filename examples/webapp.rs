#[cfg(target_arch = "wasm32")]
use egui_screensaver_matrix::MatrixBackground;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct DemoApp {
    screensaver: MatrixBackground,
}

#[cfg(target_arch = "wasm32")]
impl eframe::App for DemoApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.screensaver.paint(&ctx, frame.gl());
    }
}

#[xtask_wasm::run_example]
fn run_app() {
    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document
            .create_element("canvas")
            .expect("failed to create canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("created element is not a canvas");

        canvas
            .set_attribute(
                "style",
                "position: fixed; inset: 0; width: 100vw; height: 100vh; display: block;",
            )
            .expect("failed to set canvas style");

        document
            .body()
            .expect("no document body")
            .append_child(&canvas)
            .expect("failed to append canvas element");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(DemoApp::default()))),
            )
            .await
            .expect("failed to start eframe");
    });
}
