//! BioModels Viewer: search the curated BioModels and look at a model's
//! statistics, Antimony and SBML. The page is `app`; the `model_stats` tool
//! (src/bin) shares `stats` with it.

pub mod analytics;
pub mod antimony;
pub mod app;
pub mod cache;
pub mod search;
pub mod stats;
