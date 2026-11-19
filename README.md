# BioModels Viewer

Search the curated [BioModels](https://www.ebi.ac.uk/biomodels/) and look at a
model's metadata, statistics, Antimony and SBML. It runs entirely in the
browser (Rust compiled to WebAssembly with [Leptos](https://leptos.dev)) and is
hosted on GitHub Pages: https://sys-bio.github.io/BioModelsViewer/

## Where the data comes from

- **Search:** the metadata index in
  [sys-bio/biomodels_cache](https://github.com/sys-bio/biomodels_cache)
  (`cache/biomodels_cache.json`), downloaded once and searched in the page.
- **Models:** the SBML files in
  [sys-bio/BiomodelsStore](https://github.com/sys-bio/BiomodelsStore).
- **Files made at deploy time:** the workflow clones the store and runs the
  `model_stats` tool (`src/bin/model_stats.rs`), which publishes beside the
  page `model_stats.json` (every model's statistics, for the search filters)
  and `sbml_files.json` (model id to SBML file name; the names vary). The site
  rebuilds weekly to keep both current. Run locally, the page reads them from
  the published site.
- **Reading SBML and writing Antimony:** `sbml-io` and `mc-core` from
  [sys-bio/ModelChecker](https://github.com/sys-bio/ModelChecker), pinned to a
  commit in `Cargo.toml`.

A model can be linked directly: `https://sys-bio.github.io/BioModelsViewer/#BIOMD0000000052`.

## Search

| Query | Finds models that... |
|---|---|
| `glycolysis yeast` | contain every word, in any field |
| `"cell cycle"` | contain the phrase |
| `52`, `BIOMD52` | are BIOMD0000000052 (first), or contain 52 |
| `author:tyson`, `a:tyson` | have a matching author |
| `journal:biophys`, `j:` | appeared in a matching journal |
| `title:...`, `name:...` | match in the publication title or model name |
| `year:2005`, `y:2000-2010`, `y:>2015` | are from that year or range |
| `-calcium` | do not contain the word |
| `insulin or glucose` | match either side of `or` |

The **No assignment rules** and **No events** checkboxes keep only models
without them; models whose SBML file cannot be read are then left out. Other
filters on the statistics are one line each (`FILTERS` in `src/app.rs`).

Case, accents, and commas or semicolons around words are ignored; words of
five letters or more tolerate a typo.

## Building

Needs Rust, the `wasm32-unknown-unknown` target and [Trunk](https://trunkrs.dev):

```
rustup target add wasm32-unknown-unknown
cargo install trunk
trunk serve --open        # development server with reload
trunk build --release     # into dist/
cargo test                # unit tests (native)
```

Run the page's model loading over a folder of SBML files, to catch a panic
(which would stop the page) before it ships:

```
BIOMODELS_DIR=path/to/sbml cargo test corpus -- --ignored --nocapture
```

## Deployment

Pushing to `main` runs `.github/workflows/deploy.yml`, which tests, builds and
publishes to GitHub Pages. In the repository's Settings > Pages, set
**Source** to **GitHub Actions**.
