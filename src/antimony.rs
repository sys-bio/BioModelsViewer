//! Antimony text for the viewer, from ModelChecker's writer (mc-core).
//!
//! mc-core writes Antimony for ModelChecker's editor, so its comments talk
//! about editing: a header saying what is kept "when you edit", and
//! "(not editable here)" remarks. The viewer has no editor, so those are
//! rewritten; everything else is left as mc-core wrote it.

/// The model as Antimony, worded for reading rather than editing.
pub fn to_antimony(model: &sbml_io::Model, namespaces: &[(Option<String>, String)]) -> String {
    for_viewer(&mc_core::antimony_out::to_antimony(model, namespaces).text)
}

/// The header mc-core puts at the top, and what replaces it.
const EDITOR_HEADER: &str = "// Written from SBML by Model Checker.";
const VIEWER_HEADER: &str =
    "// Converted from SBML. Local parameters are written as reaction_name (J1_k1).";

/// Rewrites mc-core's editor comments for the viewer.
pub fn for_viewer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with(EDITOR_HEADER) {
            // The header runs on over the following comment lines.
            while lines.peek().is_some_and(|l| l.starts_with("// ")) {
                lines.next();
            }
            out.push_str(VIEWER_HEADER);
        } else {
            out.push_str(&strip_not_editable(line));
        }
        out.push('\n');
    }
    out
}

/// `(not editable here)` goes; `(not editable here: reason)` keeps the reason.
fn strip_not_editable(line: &str) -> String {
    line.replace(" (not editable here)", "")
        .replace("(not editable here: ", "(")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_editor_comments() {
        let text = "\
// Brands2002
// Written from SBML by Model Checker. Notes and the model's history are
// not shown here, but are kept when you edit. Local parameters are
// written as reaction_name (J1_k1).

model *m()
  // Units:
  // unit substance (not editable here: Antimony cannot express its definition)
  // Model units (not editable here): substance = mole
  // The model's own links (not editable here):
  // Reactions:
  J1: A -> B; k*A;
end
";
        let expected = "\
// Brands2002
// Converted from SBML. Local parameters are written as reaction_name (J1_k1).

model *m()
  // Units:
  // unit substance (Antimony cannot express its definition)
  // Model units: substance = mole
  // The model's own links:
  // Reactions:
  J1: A -> B; k*A;
end
";
        assert_eq!(for_viewer(text), expected);
    }
}
