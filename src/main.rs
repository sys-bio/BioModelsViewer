//! The page's entry point. Runs entirely in the browser.

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(biomodels_viewer::app::App);
}
