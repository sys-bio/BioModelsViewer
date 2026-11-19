//! What a model contains: counts of its parts, and what kind of model it is.

use sbml_io::{Document, Rule};

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
    /// (label, count) rows for the statistics table.
    pub counts: Vec<(&'static str, usize)>,
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
    let boundary = m.species.len() - floating;
    let (mut assignment, mut rate, mut algebraic) = (0, 0, 0);
    for rule in &m.rules {
        match rule {
            Rule::Assignment { .. } => assignment += 1,
            Rule::Rate { .. } => rate += 1,
            Rule::Algebraic { .. } => algebraic += 1,
        }
    }
    let local_parameters: usize = m
        .reactions
        .iter()
        .filter_map(|r| r.kinetic_law.as_ref())
        .map(|k| k.parameters.len())
        .sum();
    let without_rate_law = m
        .reactions
        .iter()
        .filter(|r| r.kinetic_law.as_ref().and_then(|k| k.math.as_ref()).is_none())
        .count();
    let reversible = m.reactions.iter().filter(|r| r.reversible.unwrap_or(true)).count();
    let modifiers: usize = m.reactions.iter().map(|r| r.modifiers.len()).sum();

    let has = |p: &str| packages.iter().any(|q| q == p);
    let kind = if has("fbc") {
        "Constraint-based (flux balance)"
    } else if has("qual") {
        "Logical / qualitative network"
    } else if !m.reactions.is_empty() && rate > 0 {
        "Reaction network with rate rules (ODE)"
    } else if !m.reactions.is_empty() && without_rate_law == m.reactions.len() {
        "Reaction network without rate laws"
    } else if !m.reactions.is_empty() {
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

    let counts = vec![
        ("Compartments", m.compartments.len()),
        ("Floating species", floating),
        ("Boundary species", boundary),
        ("Reactions", m.reactions.len()),
        ("  reversible", reversible),
        ("  without a rate law", without_rate_law),
        ("  modifier references", modifiers),
        ("Global parameters", m.parameters.len()),
        ("Local parameters", local_parameters),
        ("Assignment rules", assignment),
        ("Rate rules", rate),
        ("Algebraic rules", algebraic),
        ("Initial assignments", m.initial_assignments.len()),
        ("Events", m.events.len()),
        ("Function definitions", m.function_definitions.len()),
        ("Unit definitions", m.unit_definitions.len()),
        ("Constraints", m.constraints.len()),
    ];

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
        let count = |label: &str| s.counts.iter().find(|c| c.0 == label).unwrap().1;
        assert_eq!(count("Floating species"), 1);
        assert_eq!(count("Boundary species"), 1);
        assert_eq!(count("Assignment rules"), 1);
        assert_eq!(count("  reversible"), 0);
        assert_eq!(s.packages, vec!["fbc"]);
        assert!(s.kind.starts_with("Constraint-based"));
    }
}
