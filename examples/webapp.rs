#[cfg(target_arch = "wasm32")]
use egui_screensaver_matrix::{Effect, MatrixBackground, MatrixConfig, Preset};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
struct DemoApp {
    screensaver: MatrixBackground,
    logged_first_frame: bool,
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
        if !self.logged_first_frame {
            self.logged_first_frame = true;
            let rect = ctx.viewport_rect();
            log::info!(
                "egui-screensaver-matrix demo: first frame, viewport={rect:?} gl={}",
                frame.gl().is_some(),
            );
        }
        self.screensaver.paint(&ctx, frame.gl());
    }
}

/// Renders a visible error page explaining why the demo cannot start.
///
/// Without this, a missing WebGL context (privacy browsers like LibreWolf
/// ship with `webgl.disabled` and prompt per site) or a failed `eframe`
/// startup would leave nothing but an empty black canvas.
#[cfg(target_arch = "wasm32")]
fn show_error_page(title: &str, detail: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(body) = document.body() else {
        return;
    };
    let _ = body.set_attribute(
        "style",
        "margin: 0; background: #101014; color: #e8e8ec; font-family: system-ui, sans-serif;",
    );
    let div = document.create_element("div").ok();
    if let Some(div) = div {
        let title = html_escape(title);
        let detail = html_escape(detail);
        div.set_inner_html(&format!(
            "<main style=\"max-width: 40rem; margin: 20vh auto 0; padding: 0 1rem;\">\
<h1 style=\"font-size: 1.25rem;\">{title}</h1>\
<p style=\"line-height: 1.5;\">{detail}</p>\
<p style=\"line-height: 1.5; opacity: 0.8;\">If you use LibreWolf or another privacy-hardened \
browser, allow WebGL for this site (shield icon → Open site preferences → Permissions → \
Disable WebGL: off) and reload. Any further details are printed to the browser console (F12).</p>\
</main>"
        ));
        let _ = body.append_child(&div);
    }
}

#[cfg(target_arch = "wasm32")]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Probes the canvas for a usable WebGL context without giving the page one.
///
/// `eframe`'s own failure mode for "no WebGL" is a panic that most visitors
/// will never see in the console — all they get is a black page. Checking up
/// front lets us show the error page above instead.
#[cfg(target_arch = "wasm32")]
fn webgl_available(document: &web_sys::Document) -> bool {
    let Some(canvas) = document
        .create_element("canvas")
        .ok()
        .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        return false;
    };
    canvas.get_context("webgl2").is_ok_and(|gl| gl.is_some())
        || canvas.get_context("webgl").is_ok_and(|gl| gl.is_some())
}

#[xtask_wasm::run_example]
fn run_app() {
    struct ConsoleLogger;
    impl log::Log for ConsoleLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record) {
            let msg = format!(
                "[egui-screensaver-matrix:{}] {}",
                record.level(),
                record.args()
            );
            match record.level() {
                log::Level::Error => web_sys::console::error_1(&msg.into()),
                log::Level::Warn => web_sys::console::warn_1(&msg.into()),
                _ => web_sys::console::info_1(&msg.into()),
            }
        }
        fn flush(&self) {}
    }
    let _ = log::set_boxed_logger(Box::new(ConsoleLogger));
    log::set_max_level(log::LevelFilter::Info);

    #[cfg(target_arch = "wasm32")]
    {
        let web_options = eframe::WebOptions::default();

        wasm_bindgen_futures::spawn_local(async {
            let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                show_error_page(
                    "egui-screensaver-matrix demo",
                    "Could not access the DOM document.",
                );
                return;
            };

            if !webgl_available(&document) {
                log::error!("demo: no WebGL context available on the canvas");
                show_error_page(
                    "WebGL is not available",
                    "This Matrix rain demo renders with WebGL, but the browser refused to \
                     create a WebGL context.",
                );
                return;
            }

            let Some(canvas) = document
                .create_element("canvas")
                .ok()
                .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            else {
                show_error_page(
                    "egui-screensaver-matrix demo",
                    "Could not create the canvas element.",
                );
                return;
            };

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

            if let Err(err) = eframe::WebRunner::new()
                .start(
                    canvas,
                    web_options,
                    Box::new(|_cc| {
                        let mut screensaver = MatrixBackground::default();
                        screensaver.config = config_from_query();
                        Ok(Box::new(DemoApp {
                            screensaver,
                            logged_first_frame: false,
                        }))
                    }),
                )
                .await
            {
                let detail = err
                    .as_string()
                    .unwrap_or_else(|| "Unknown error starting eframe".to_owned());
                log::error!("demo: failed to start eframe: {detail}");
                show_error_page("Failed to start the demo", &detail);
            }
        });
    }
}
