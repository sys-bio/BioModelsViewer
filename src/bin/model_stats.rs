//! Writes the files the page reads beside it, from a copy of
//! sys-bio/BiomodelsStore. Run by the deploy workflow:
//!
//! ```text
//! git clone --depth 1 https://github.com/sys-bio/BiomodelsStore store
//! cargo run --release --bin model_stats -- store/biomodels dist
//! ```
//!
//! - `model_stats.json`: every model's statistics (kind and counts), so the
//!   search can filter on them;
//! - `sbml_files.json`: model id to SBML file name (the names vary).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use biomodels_viewer::stats::{Summary, summarize};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [store, out] = args.as_slice() else {
        eprintln!("usage: model_stats <BiomodelsStore/biomodels folder> <output folder>");
        std::process::exit(2);
    };
    let (stats, files) = scan(Path::new(store));
    if stats.is_empty() {
        eprintln!("no models found in {store}");
        std::process::exit(1);
    }
    let out = Path::new(out);
    std::fs::create_dir_all(out).expect("output folder");
    write_json(&out.join("model_stats.json"), &stats);
    write_json(&out.join("sbml_files.json"), &files);
    let unreadable = stats.values().filter(|s| s.unreadable).count();
    println!(
        "{} models: {} with statistics, {unreadable} unreadable",
        stats.len(),
        stats.len() - unreadable
    );
}

/// Each model folder's statistics and SBML file name, by model id.
fn scan(store: &Path) -> (BTreeMap<String, Summary>, BTreeMap<String, String>) {
    let mut stats = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut folders: Vec<PathBuf> = std::fs::read_dir(store)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", store.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        let Some(id) = folder.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let Some(file) = sbml_file(&folder) else { continue };
        let bytes = std::fs::read(&file).unwrap_or_default();
        let xml = String::from_utf8_lossy(&bytes);
        // A panic in the reader must not stop the deploy: count it unreadable.
        let summary = std::panic::catch_unwind(|| summarize(&xml)).unwrap_or_else(|_| {
            eprintln!("{id}: the reader panicked");
            Summary {
                unreadable: true,
                ..Default::default()
            }
        });
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        files.insert(id.clone(), name);
        stats.insert(id, summary);
    }
    (stats, files)
}

/// The folder's SBML file: its one `.xml` file, or the first by name.
fn sbml_file(folder: &Path) -> Option<PathBuf> {
    let mut xml: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")))
        .collect();
    xml.sort();
    xml.into_iter().next()
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) {
    let text = serde_json::to_string(value).expect("serialize");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}
