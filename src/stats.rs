//! What a model contains: counts of its parts, and what kind of model it is.
//!
//! The page works these out for the model it shows. The `model_stats` tool
//! works them out for every model in the store at deploy time, as
//! [`Summary`]s, so the search can filter on them.

use sbml_io::{Document, Rule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelStats {
    pub level: u32,
    pub version: u32,
    /// SBML Level 3 packages declared, such as `fbc`, `qual`, `layout`.
    pub packages: Vec<String>,
    /// A one-line description of what kind of model it is.
    pub kind: String,
    /// Notable features: events, delays, algebraic rules and so on.
    pub features: Vec<&'static str>,
    pub counts: Counts,
}

/// How many of each part a model has. Saved in `model_stats.json`, where
/// zeros are left out (so a field added later reads as 0 from an old file).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counts {
    #[serde(skip_serializing_if = "is_zero")]
    pub compartments: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub floating_species: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub boundary_species: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub reactions: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub reversible_reactions: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub reactions_without_rate_law: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub modifier_references: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub global_parameters: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub local_parameters: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub assignment_rules: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub rate_rules: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub algebraic_rules: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub initial_assignments: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub events: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub function_definitions: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub unit_definitions: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub constraints: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl Counts {
    /// (label, count) rows for the statistics table; an indented label is a
    /// detail of the row above.
    pub fn rows(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("Compartments", self.compartments),
            ("Floating species", self.floating_species),
            ("Boundary species", self.boundary_species),
            ("Reactions", self.reactions),
            ("  reversible", self.reversible_reactions),
            ("  without a rate law", self.reactions_without_rate_law),
            ("  modifier references", self.modifier_references),
            ("Global parameters", self.global_parameters),
            ("Local parameters", self.local_parameters),
            ("Assignment rules", self.assignment_rules),
            ("Rate rules", self.rate_rules),
            ("Algebraic rules", self.algebraic_rules),
            ("Initial assignments", self.initial_assignments),
            ("Events", self.events),
            ("Function definitions", self.function_definitions),
            ("Unit definitions", self.unit_definitions),
            ("Constraints", self.constraints),
        ]
    }
}

/// One model's entry in `model_stats.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    /// The file is not SBML, or could not be read; then nothing else is set.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unreadable: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub counts: Counts,
}

/// The statistics of an SBML file, for `model_stats.json`.
pub fn summarize(xml: &str) -> Summary {
    let unreadable = Summary {
        unreadable: true,
        ..Default::default()
    };
    if !xml.contains("<sbml") {
        return unreadable;
    }
    match sbml_io::read(xml) {
        Ok(read) => {
            let stats = analyze(&read.document, xml);
            Summary {
                unreadable: false,
                kind: stats.kind,
                counts: stats.counts,
            }
        }
        Err(_) => unreadable,
    }
}

/// Works out a model's statistics. `xml` is the file the document was read
/// from, searched for MathML features (`delay`) that are simpler to find in
/// the text than in the tree.
pub fn analyze(doc: &Document, xml: &str) -> ModelStats {
    let packages = packages(&doc.namespaces);
    let empty = sbml_io::Model::default();
    let m = doc.model.as_ref().unwrap_or(&empty);

    let is_boundary = |s: &sbml_io::Species| {
        s.boundary_condition.unwrap_or(false) || s.constant.unwrap_or(false)
    };
    let floating = m.species.iter().filter(|s| !is_boundary(s)).count();
    let (mut assignment, mut rate, mut algebraic) = (0, 0, 0);
    for rule in &m.rules {
        match rule {
            Rule::Assignment { .. } => assignment += 1,
            Rule::Rate { .. } => rate += 1,
            Rule::Algebraic { .. } => algebraic += 1,
        }
    }
    let counts = Counts {
        compartments: m.compartments.len(),
        floating_species: floating,
        boundary_species: m.species.len() - floating,
        reactions: m.reactions.len(),
        reversible_reactions: m
            .reactions
            .iter()
            .filter(|r| r.reversible.unwrap_or(true))
            .count(),
        reactions_without_rate_law: m
            .reactions
            .iter()
            .filter(|r| r.kinetic_law.as_ref().and_then(|k| k.math.as_ref()).is_none())
            .count(),
        modifier_references: m.reactions.iter().map(|r| r.modifiers.len()).sum(),
        global_parameters: m.parameters.len(),
        local_parameters: m
            .reactions
            .iter()
            .filter_map(|r| r.kinetic_law.as_ref())
            .map(|k| k.parameters.len())
            .sum(),
        assignment_rules: assignment,
        rate_rules: rate,
        algebraic_rules: algebraic,
        initial_assignments: m.initial_assignments.len(),
        events: m.events.len(),
        function_definitions: m.function_definitions.len(),
        unit_definitions: m.unit_definitions.len(),
        constraints: m.constraints.len(),
    };

    let has = |p: &str| packages.iter().any(|q| q == p);
    let reactions = counts.reactions;
    let kind = if has("fbc") {
        "Constraint-based (flux balance)"
    } else if has("qual") {
        "Logical / qualitative network"
    } else if reactions > 0 && rate > 0 {
        "Reaction network with rate rules (ODE)"
    } else if reactions > 0 && counts.reactions_without_rate_law == reactions {
        "Reaction network without rate laws"
    } else if reactions > 0 {
        "Kinetic reaction network (ODE)"
    } else if rate > 0 {
        "ODE model written as rate rules"
    } else if assignment > 0 || algebraic > 0 {
        "Algebraic model (rules only, no dynamics)"
    } else {
        "No reactions or rules"
    }
    .to_string();

    let mut features = Vec::new();
    if !m.events.is_empty() {
        features.push("Events (discontinuities)");
    }
    if algebraic > 0 {
        features.push("Algebraic rules (DAE)");
    }
    if xml.contains("http://www.sbml.org/sbml/symbols/delay") {
        features.push("Delays");
    }
    if m.compartments.len() > 1 {
        features.push("Several compartments");
    }
    if !m.function_definitions.is_empty() {
        features.push("Function definitions");
    }
    if xml.contains("<piecewise") {
        features.push("Piecewise expressions");
    }
    if m.species.iter().any(|s| s.has_only_substance_units == Some(true)) {
        features.push("Species in amounts");
    }

    ModelStats {
        level: doc.level,
        version: doc.version,
        packages,
        kind,
        features,
        counts,
    }
}

/// Level 3 package names from the namespace URIs, such as
/// `http://www.sbml.org/sbml/level3/version1/fbc/version2` giving `fbc`.
fn packages(namespaces: &[(Option<String>, String)]) -> Vec<String> {
    let mut out: Vec<String> = namespaces
        .iter()
        .filter_map(|(_, uri)| {
            let rest = uri.strip_prefix("http://www.sbml.org/sbml/level3/")?;
            let name = rest.split('/').nth(1)?;
            (!name.is_empty() && name != "core").then(|| name.to_string())
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1"
      xmlns:fbc="http://www.sbml.org/sbml/level3/version1/fbc/version2" fbc:required="false">
  <model id="m">
    <listOfCompartments>
      <compartment id="cell" size="1" constant="true"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="X" compartment="cell" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="true" constant="false"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="k" value="1" constant="true"/>
      <parameter id="y" value="1" constant="false"/>
    </listOfParameters>
    <listOfRules>
      <assignmentRule variable="y">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><ci>k</ci></math>
      </assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="J1" reversible="false">
        <listOfReactants><speciesReference species="X" stoichiometry="1" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="A" stoichiometry="1" constant="true"/></listOfProducts>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><ci>k</ci></math>
        </kineticLaw>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;

    #[test]
    fn counts_species_rules_and_packages() {
        let doc = sbml_io::read(XML).unwrap().document;
        let s = analyze(&doc, XML);
        assert_eq!(s.counts.floating_species, 1);
        assert_eq!(s.counts.boundary_species, 1);
        assert_eq!(s.counts.assignment_rules, 1);
        assert_eq!(s.counts.reversible_reactions, 0);
        assert_eq!(s.packages, vec!["fbc"]);
        assert!(s.kind.starts_with("Constraint-based"));
    }

    #[test]
    fn summaries_leave_out_zeros_and_read_back() {
        let summary = summarize(XML);
        let json = serde_json::to_string(&summary).unwrap();
        assert!(!json.contains("events"), "{json}");
        assert!(json.contains(r#""assignment_rules":1"#), "{json}");
        assert_eq!(serde_json::from_str::<Summary>(&json).unwrap(), summary);
        let page = summarize("<!doctype html><html></html>");
        assert!(page.unreadable);
        assert_eq!(
            serde_json::to_string(&page).unwrap(),
            r#"{"unreadable":true,"counts":{}}"#
        );
    }
}
