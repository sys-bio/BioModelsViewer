//! The user interface: a search box, the list of matching models, and the
//! selected model's details, statistics, Antimony and SBML.

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::cache::{self, ModelEntry};
use crate::search::SearchIndex;
use crate::stats::{self, ModelStats};

/// At most this many results are listed; the count says how many matched.
const RESULT_LIMIT: usize = 250;

#[derive(Clone)]
enum Status<T> {
    Loading,
    Ready(T),
    Failed(String),
}

struct Catalog {
    entries: Vec<ModelEntry>,
    index: SearchIndex,
}

/// A downloaded model, read and converted.
struct LoadedModel {
    /// Where the file was downloaded from.
    url: String,
    xml: String,
    /// Why the SBML could not be read, if it could not.
    read_error: Option<String>,
    read_warnings: Vec<String>,
    stats: Option<ModelStats>,
    antimony: Result<String, String>,
}

impl LoadedModel {
    fn from_xml(url: String, xml: String) -> Self {
        // A few of the store's files hold a web page saved in place of the SBML.
        if !xml.contains("<sbml") {
            let message = "The store's file for this model is not SBML (it holds a web page \
                 saved in its place). The model is still available from BioModels itself."
                .to_string();
            return LoadedModel {
                read_error: Some(message.clone()),
                read_warnings: Vec::new(),
                stats: None,
                antimony: Err(message),
                url,
                xml,
            };
        }
        match sbml_io::read(&xml) {
            Ok(read) => {
                let stats = stats::analyze(&read.document, &xml);
                let antimony = match &read.document.model {
                    Some(model) => Ok(crate::antimony::to_antimony(
                        model,
                        &read.document.namespaces,
                    )),
                    None => Err("The file has no model element.".to_string()),
                };
                LoadedModel {
                    read_error: None,
                    read_warnings: read.warnings.iter().map(ToString::to_string).collect(),
                    stats: Some(stats),
                    antimony,
                    url,
                    xml,
                }
            }
            Err(diagnostics) => {
                let message = diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n");
                LoadedModel {
                    read_error: Some(message.clone()),
                    read_warnings: Vec::new(),
                    stats: None,
                    antimony: Err(message),
                    url,
                    xml,
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Statistics,
    Antimony,
    Sbml,
}

/// One line of the result list.
#[derive(Clone, PartialEq)]
struct Row {
    index: usize,
    id: String,
    name: String,
    detail: String,
}

impl Row {
    fn new(index: usize, e: &ModelEntry) -> Self {
        let authors = match e.authors.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [first, ..] => format!("{first} et al."),
        };
        let detail = [authors, e.journal.clone(), e.date.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" - ");
        Row {
            index,
            id: e.id.clone(),
            name: if e.name.is_empty() { e.title.clone() } else { e.name.clone() },
            detail,
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let catalog = RwSignal::new(Status::<Arc<Catalog>>::Loading);
    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<usize>);
    let model = RwSignal::new(Status::<Arc<LoadedModel>>::Loading);
    let tab = RwSignal::new(Tab::Statistics);
    let show_help = RwSignal::new(false);
    let show_about = RwSignal::new(false);

    // Escape closes the About box.
    let _ = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            show_about.set(false);
        }
    });
    let manifest = RwSignal::new(None::<Arc<cache::Manifest>>);

    spawn_local(async move {
        if let Some(m) = cache::fetch_manifest().await {
            manifest.set(Some(Arc::new(m)));
        }
    });

    // Load the index once, then open the model named in the URL (#BIOMD...).
    spawn_local(async move {
        let loaded = cache::fetch_text(cache::INDEX_URL)
            .await
            .and_then(|text| cache::parse_index(&text));
        match loaded {
            Ok(entries) => {
                let wanted = hash_model_id();
                let index = SearchIndex::new(&entries);
                let start = wanted.and_then(|id| entries.iter().position(|e| e.id == id));
                catalog.set(Status::Ready(Arc::new(Catalog { entries, index })));
                if start.is_some() {
                    selected.set(start);
                }
            }
            Err(e) => catalog.set(Status::Failed(e)),
        }
    });

    let matches = Memo::new(move |_| match catalog.get() {
        Status::Ready(cat) => query.with(|q| cat.index.search(q)),
        _ => Vec::new(),
    });
    let rows = Memo::new(move |_| match catalog.get() {
        Status::Ready(cat) => matches.with(|m| {
            m.iter()
                .take(RESULT_LIMIT)
                .map(|&i| Row::new(i, &cat.entries[i]))
                .collect::<Vec<_>>()
        }),
        _ => Vec::new(),
    });

    // Download the selected model's SBML.
    Effect::new(move |_| {
        let Some(i) = selected.get() else { return };
        let id = match catalog.get_untracked() {
            Status::Ready(cat) => cat.entries[i].id.clone(),
            _ => return,
        };
        set_hash(&id);
        model.set(Status::Loading);
        spawn_local(async move {
            let files = manifest.get_untracked();
            let result = cache::fetch_sbml(&id, files.as_deref()).await;
            if selected.get_untracked() != Some(i) {
                return; // another model was chosen meanwhile
            }
            model.set(match result {
                Ok((xml, url)) => Status::Ready(Arc::new(LoadedModel::from_xml(url, xml))),
                Err(e) => Status::Failed(e),
            });
        });
    });

    let result_list = move || match catalog.get() {
        Status::Loading => view! { <p class="note">"Loading the model index..."</p> }.into_any(),
        Status::Failed(e) => view! { <p class="error">{e}</p> }.into_any(),
        Status::Ready(cat) => {
            let total = cat.entries.len();
            view! {
                <p class="count">
                    {move || {
                        let n = matches.with(Vec::len);
                        let shown = if n > RESULT_LIMIT {
                            format!(" (first {RESULT_LIMIT} shown)")
                        } else {
                            String::new()
                        };
                        format!("{n} of {total} models{shown}")
                    }}
                </p>
                <ul class="result-list">
                    <For
                        each=move || rows.get()
                        key=|row| row.index
                        children=move |row| {
                            let index = row.index;
                            view! {
                                <li
                                    class="result"
                                    class:selected=move || selected.get() == Some(index)
                                    on:click=move |_| selected.set(Some(index))
                                >
                                    <div class="result-name">{row.name}</div>
                                    <div class="result-id">{row.id}</div>
                                    <div class="result-detail">{row.detail}</div>
                                </li>
                            }
                        }
                    />
                </ul>
            }
            .into_any()
        }
    };

    let detail = move || {
        let entry = selected.get().and_then(|i| match catalog.get() {
            Status::Ready(cat) => cat.entries.get(i).cloned(),
            _ => None,
        });
        match entry {
            None => view! {
                <div class="placeholder">
                    <p>"Search for a model on the left and select it to see its details."</p>
                    <p>"Some searches to try (click one):"</p>
                    <p class="examples">
                        {EXAMPLES.iter().map(|&example| view! {
                            <button class="example" on:click=move |_| query.set(example.to_string())>
                                <code>{example}</code>
                            </button>
                        }).collect_view()}
                    </p>
                    <p class="examples">
                        <span>"Combine them:"</span>
                        {COMBINED.iter().map(|&example| view! {
                            <button class="example" on:click=move |_| query.set(example.to_string())>
                                <code>{example}</code>
                            </button>
                        }).collect_view()}
                    </p>
                </div>
            }
            .into_any(),
            Some(entry) => view! { <ModelDetail entry=entry model=model tab=tab /> }.into_any(),
        }
    };

    view! {
        <header class="top">
            <h1>"BioModels Viewer" <span class="version">{format!("v{VERSION}")}</span></h1>
            <input
                class="search"
                type="search"
                placeholder="Search models: words, an id, author:..., journal:..., year:..."
                autofocus
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            <button class="link-button" on:click=move |_| show_help.update(|v| *v = !*v)>
                {move || if show_help.get() { "Hide help" } else { "Search help" }}
            </button>
            <button class="link-button" on:click=move |_| show_about.set(true)>"About"</button>
        </header>
        <Show when=move || show_about.get()>
            <About
                models=Signal::derive(move || match catalog.get() {
                    Status::Ready(cat) => Some(cat.entries.len()),
                    _ => None,
                })
                on_close=move || show_about.set(false)
            />
        </Show>
        <Show when=move || show_help.get()>
            <SearchHelp />
        </Show>
        <main class="layout">
            <section class="results">{result_list}</section>
            <section class="detail">{detail}</section>
        </main>
    }
}

/// Searches offered on the start page, each a button that runs it.
const EXAMPLES: [&str; 5] = [
    "glycolysis",
    "52",
    "author:tyson",
    "calcium year:>2010",
    "insulin or glucose",
];

/// Searches showing how pieces combine: spaces for "and", `or` for either.
const COMBINED: [&str; 2] = ["glycolysis author:teusink", "calcium or glycolysis"];

/// The version in Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit the page was built from: GitHub Actions sets `GITHUB_SHA`.
fn build_commit() -> Option<&'static str> {
    option_env!("GITHUB_SHA").map(|sha| &sha[..sha.len().min(7)])
}

#[component]
fn About(models: Signal<Option<usize>>, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let repo = "https://github.com/sys-bio/BioModelsViewer";
    let build = build_commit().map(|sha| {
        view! {
            " (build "
            <a href=format!("{repo}/commit/{sha}") target="_blank" rel="noopener">{sha}</a>
            ")"
        }
    });
    view! {
        <div class="overlay" on:click=move |_| on_close()>
            <div
                class="about"
                role="dialog"
                aria-modal="true"
                aria-labelledby="about-title"
                on:click=|ev| ev.stop_propagation()
            >
                <h2 id="about-title">"BioModels Viewer"</h2>
                <p class="about-version">"Version " {VERSION} {build}</p>
                <p>
                    "Search the curated models of the "
                    <a href="https://www.ebi.ac.uk/biomodels/" target="_blank" rel="noopener">"BioModels"</a>
                    " database and look at each model's statistics, Antimony and SBML. "
                    "Everything runs in your browser."
                </p>
                <p>
                    {move || match models.get() {
                        Some(n) => format!("{n} models in the index."),
                        None => "The model index is loading.".to_string(),
                    }}
                </p>
                <h3>"Data"</h3>
                <ul>
                    <li>
                        "Model metadata: "
                        <a href="https://github.com/sys-bio/biomodels_cache" target="_blank" rel="noopener">"sys-bio/biomodels_cache"</a>
                    </li>
                    <li>
                        "SBML files: "
                        <a href="https://github.com/sys-bio/BiomodelsStore" target="_blank" rel="noopener">"sys-bio/BiomodelsStore"</a>
                    </li>
                    <li>
                        "SBML reading and Antimony conversion: sbml-io and mc-core from "
                        <a href="https://github.com/sys-bio/ModelChecker" target="_blank" rel="noopener">"sys-bio/ModelChecker"</a>
                    </li>
                </ul>
                <h3>"Software"</h3>
                <p>
                    "Written in Rust with "
                    <a href="https://leptos.dev" target="_blank" rel="noopener">"Leptos"</a>
                    ", compiled to WebAssembly. Source code: "
                    <a href=repo target="_blank" rel="noopener">"sys-bio/BioModelsViewer"</a>
                    ". MIT licence."
                </p>
                <h3>"Funding"</h3>
                <p class="funding">
                    "This work was supported by NIH Biomedical Imaging and Bioengineering award "
                    "P41 EB023912 through HMS at the Center for Reproducible Biomedical Modeling "
                    <a href="https://reproduciblebiomodels.org" target="_blank" rel="noopener">"reproduciblebiomodels.org"</a>
                </p>
                <p class="funding">
                    "This project has been funded in whole or in part with Federal funds from the "
                    "Advanced Research Projects Agency for Health (ARPA-H), Department of Health and "
                    "Human Services, under the Critical Illness Immunological Reprogramming and "
                    "Control Point Learning Engine (CIRCLE) program. The views and conclusions "
                    "contained in this document are those of the authors and should not be "
                    "interpreted as representing the official policies, either expressed or "
                    "implied, of the U.S. Government."
                </p>
                <div class="about-actions">
                    <button class="button" on:click=move |_| on_close()>"Close"</button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SearchHelp() -> impl IntoView {
    view! {
        <div class="help">
            <table>
                <tr><td><code>"glycolysis yeast"</code></td><td>"every word, in any field"</td></tr>
                <tr><td><code>"\"cell cycle\""</code></td><td>"the exact phrase"</td></tr>
                <tr><td><code>"52"</code>", "<code>"BIOMD52"</code></td><td>"model BIOMD0000000052 first"</td></tr>
                <tr><td><code>"author:tyson"</code>", "<code>"a:tyson"</code></td><td>"an author"</td></tr>
                <tr><td><code>"journal:biophys"</code>", "<code>"j:"</code></td><td>"a journal"</td></tr>
                <tr><td><code>"title:"</code>", "<code>"name:"</code></td><td>"the publication title or the model name"</td></tr>
                <tr><td><code>"year:2005"</code>", "<code>"y:2000-2010"</code>", "<code>"y:>2015"</code></td><td>"a year or range"</td></tr>
                <tr><td><code>"-calcium"</code></td><td>"leave out models with the word"</td></tr>
                <tr><td><code>"insulin or glucose"</code></td><td>"either side of " <code>"or"</code></td></tr>
            </table>
            <p>"Case and accents are ignored, and longer words tolerate a typo or two."</p>
        </div>
    }
}

#[component]
fn ModelDetail(
    entry: ModelEntry,
    model: RwSignal<Status<Arc<LoadedModel>>>,
    tab: RwSignal<Tab>,
) -> impl IntoView {
    let id = entry.id.clone();
    let tab_button = move |which: Tab, label: &'static str| {
        view! {
            <button
                class="tab"
                class:active=move || tab.get() == which
                on:click=move |_| tab.set(which)
            >
                {label}
            </button>
        }
    };

    let body = move || match model.get() {
        Status::Loading => view! { <p class="note">"Downloading the SBML file..."</p> }.into_any(),
        Status::Failed(e) => view! { <p class="error">{e}</p> }.into_any(),
        Status::Ready(m) => match tab.get() {
            Tab::Statistics => statistics_view(&m).into_any(),
            Tab::Antimony => match &m.antimony {
                Ok(text) => code_view(text.clone()).into_any(),
                Err(e) => view! { <p class="error">"No Antimony: " {e.clone()}</p> }.into_any(),
            },
            Tab::Sbml => code_view(m.xml.clone()).into_any(),
        },
    };

    view! {
        <article class="model">
            <h2>{entry.name.clone()}</h2>
            <p class="ids">
                <a href=cache::biomodels_page(&id) target="_blank" rel="noopener">{id.clone()}</a>
                {move || match model.get() {
                    Status::Ready(m) => Some(view! {
                        " - "
                        <a href=m.url.clone() target="_blank" rel="noopener">"SBML file"</a>
                    }),
                    _ => None,
                }}
            </p>
            <dl class="meta">
                <dt>"Title"</dt>
                <dd>
                    {match entry.url.clone() {
                        Some(url) => view! { <a href=url target="_blank" rel="noopener">{entry.title.clone()}</a> }.into_any(),
                        None => view! { <span>{entry.title.clone()}</span> }.into_any(),
                    }}
                </dd>
                <dt>"Authors"</dt>
                <dd>{entry.authors.join(", ")}</dd>
                <dt>"Journal"</dt>
                <dd>{format!("{} ({})", entry.journal, entry.date)}</dd>
                {entry.citation.clone().map(|c| view! { <dt>"Citation"</dt><dd>{c}</dd> })}
            </dl>
            <details class="synopsis">
                <summary>"Synopsis"</summary>
                <p>{entry.synopsis.clone()}</p>
            </details>
            <nav class="tabs">
                {tab_button(Tab::Statistics, "Statistics")}
                {tab_button(Tab::Antimony, "Antimony")}
                {tab_button(Tab::Sbml, "SBML")}
            </nav>
            <div class="tab-body">{body}</div>
        </article>
    }
}

fn statistics_view(m: &LoadedModel) -> impl IntoView + use<> {
    if let Some(error) = &m.read_error {
        return view! {
            <div>
                <p class="error">"The SBML file could not be read:"</p>
                <pre class="code">{error.clone()}</pre>
            </div>
        }
        .into_any();
    }
    let Some(s) = m.stats.clone() else {
        return view! { <p class="note">"No statistics."</p> }.into_any();
    };
    let packages = if s.packages.is_empty() {
        "none".to_string()
    } else {
        s.packages.join(", ")
    };
    let features = if s.features.is_empty() {
        "none".to_string()
    } else {
        s.features.join(", ")
    };
    let warnings = m.read_warnings.clone();
    view! {
        <div class="stats">
            <dl class="meta">
                <dt>"Kind"</dt><dd><strong>{s.kind.clone()}</strong></dd>
                <dt>"SBML"</dt><dd>{format!("Level {} Version {}", s.level, s.version)}</dd>
                <dt>"Packages"</dt><dd>{packages}</dd>
                <dt>"Features"</dt><dd>{features}</dd>
            </dl>
            <table class="counts">
                {s.counts.iter().map(|(label, n)| {
                    let sub = label.starts_with(' ');
                    view! {
                        <tr class:sub=sub class:zero=*n == 0>
                            <td>{label.trim()}</td>
                            <td class="num">{*n}</td>
                        </tr>
                    }
                }).collect_view()}
            </table>
            {(!warnings.is_empty()).then(|| view! {
                <details class="warnings">
                    <summary>{format!("{} reader warnings", warnings.len())}</summary>
                    <pre class="code">{warnings.join("\n")}</pre>
                </details>
            })}
        </div>
    }
    .into_any()
}

/// A block of text with a button that copies it.
fn code_view(text: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    let to_copy = text.clone();
    view! {
        <div class="code-block">
            <button class="copy" on:click=move |_| {
                if let Some(w) = web_sys::window() {
                    let _ = w.navigator().clipboard().write_text(&to_copy);
                    copied.set(true);
                }
            }>
                {move || if copied.get() { "Copied" } else { "Copy" }}
            </button>
            <pre class="code">{text}</pre>
        </div>
    }
}

/// The model id in the page address (`...#BIOMD0000000052`), if any.
fn hash_model_id() -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let id = hash.trim_start_matches('#').trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// Puts the model id in the page address, so it can be bookmarked or shared,
/// without adding a history entry for every model looked at.
fn set_hash(id: &str) {
    if let Some(w) = web_sys::window()
        && let Ok(history) = w.history()
    {
        let _ = history.replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(&format!("#{id}")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::LoadedModel;

    /// Reads every `.xml` file in the folder named by `BIOMODELS_DIR` the way
    /// the page does (read, statistics, Antimony), so a panic, which would
    /// stop the page, shows up here first. Run with
    /// `BIOMODELS_DIR=... cargo test corpus -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn corpus() {
        let dir = std::env::var("BIOMODELS_DIR").expect("set BIOMODELS_DIR to a folder of SBML files");
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "xml"))
            .collect();
        paths.sort();
        let (mut ok, mut unreadable, mut no_antimony, mut panics) = (0, 0, 0, Vec::new());
        for path in &paths {
            let xml = std::fs::read_to_string(path).unwrap();
            match std::panic::catch_unwind(|| LoadedModel::from_xml(String::new(), xml)) {
                Ok(m) if m.read_error.is_some() => unreadable += 1,
                Ok(m) if m.antimony.is_err() => no_antimony += 1,
                Ok(m) if m.antimony.as_ref().is_ok_and(|a| {
                    a.contains("Model Checker") || a.contains("editable")
                }) =>
                {
                    panics.push(format!("{}: editor wording left in the Antimony", path.display()))
                }
                Ok(_) => ok += 1,
                Err(_) => panics.push(path.display().to_string()),
            }
        }
        println!(
            "{} files: {ok} ok, {unreadable} unreadable, {no_antimony} without Antimony, {} panicked",
            paths.len(),
            panics.len()
        );
        assert!(panics.is_empty(), "panicked on: {panics:#?}");
    }
}
