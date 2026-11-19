//! BioModels Viewer: search the curated BioModels and look at a model's
//! statistics, Antimony and SBML. Runs entirely in the browser.

mod app;
mod cache;
mod search;
mod stats;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
