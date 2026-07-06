#[cfg(target_arch = "wasm32")]
use egui_screensaver_matrix::{Effect, MatrixBackground, MatrixConfig, Preset};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
struct DemoApp {
    screensaver: MatrixBackground,
}

/// Reads `?preset=NAME` (and optional `&effect=pride|trans`) from the page
/// URL, so the demo page can deep-link to any preset. Mirrors the original's
/// `?version=NAME` demo links, renamed to match this crate's `Preset` type.
#[cfg(target_arch = "wasm32")]
fn config_from_query() -> MatrixConfig {
    let params = web_sys::UrlSearchParams::new_with_str(
        &web_sys::window()
            .expect("no window")
            .location()
            .search()
            .expect("no location.search"),
    )
    .expect("failed to parse query string");

    let preset = match params.get("preset").as_deref() {
        Some("classic") | None => Preset::Classic,
        Some("megacity") => Preset::Megacity,
        Some("neomatrixology") => Preset::Neomatrixology,
        Some("operator") => Preset::Operator,
        Some("nightmare") => Preset::Nightmare,
        Some("paradise") => Preset::Paradise,
        Some("resurrections") => Preset::Resurrections,
        Some("palimpsest") => Preset::Palimpsest,
        Some("twilight") => Preset::Twilight,
        Some("trinity") => Preset::Trinity,
        Some("morpheus") => Preset::Morpheus,
        Some("bugs") => Preset::Bugs,
        Some("3d") => Preset::ThreeD,
        Some(other) => {
            log::warn!("egui-screensaver-matrix demo: unknown ?preset={other}, using classic");
            Preset::Classic
        }
    };

    let mut config = MatrixConfig::from_preset(preset);
    match params.get("effect").as_deref() {
        Some("pride") => config.effect = Effect::Pride,
        Some("trans") => config.effect = Effect::Trans,
        Some(other) => {
            log::warn!("egui-screensaver-matrix demo: unknown &effect={other}, ignoring")
        }
        None => {}
    }
    config
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
                Box::new(|_cc| {
                    let mut screensaver = MatrixBackground::default();
                    screensaver.config = config_from_query();
                    Ok(Box::new(DemoApp { screensaver }))
                }),
            )
            .await
            .expect("failed to start eframe");
    });
}
