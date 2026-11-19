//! Searching the model index, entirely in the browser.
//!
//! Query syntax (case and accents are ignored):
//!
//! | Query                  | Finds models that...                                  |
//! |------------------------|-------------------------------------------------------|
//! | `glycolysis yeast`     | contain every word, in any field                      |
//! | `"cell cycle"`         | contain the phrase                                    |
//! | `52`, `BIOMD52`        | are BIOMD0000000052 (listed first), or contain 52     |
//! | `author:tyson`, `a:`   | have an author matching                               |
//! | `journal:biophys`, `j:`| were published in a matching journal                  |
//! | `title:`, `name:`      | match in the publication title or model name         |
//! | `year:2005`, `y:`      | are from 2005; also `2000-2010`, `>2015`, `<1995`     |
//! | `-calcium`             | do not contain the word                               |
//!
//! A word of five letters or more that matches nothing exactly also matches
//! words one typo away (two for long words), ranked below exact matches.

use crate::cache::ModelEntry;

/// One model's fields, folded for matching.
struct Folded {
    number: Option<u64>,
    id: String,
    name: String,
    title: String,
    authors: String,
    journal: String,
    synopsis: String,
    year: Option<i32>,
    /// The distinct words of the name, title, authors and journal, for typo matching.
    words: Vec<String>,
}

pub struct SearchIndex {
    items: Vec<Folded>,
}

#[derive(Debug, Clone, PartialEq)]
enum Field {
    Any,
    Name,
    Title,
    Author,
    Journal,
    Id,
}

#[derive(Debug, Clone, PartialEq)]
enum Term {
    Text { field: Field, text: String, negated: bool },
    Year { from: i32, to: i32 },
}

impl SearchIndex {
    pub fn new(entries: &[ModelEntry]) -> Self {
        let items = entries
            .iter()
            .map(|e| {
                let name = fold(&e.name);
                let title = fold(&e.title);
                let authors = fold(&e.authors.join("; "));
                let journal = fold(&e.journal);
                let mut words: Vec<String> = [&name, &title, &authors, &journal]
                    .iter()
                    .flat_map(|s| s.split(|c: char| !c.is_alphanumeric()))
                    .filter(|w| w.chars().count() >= 4)
                    .map(str::to_string)
                    .collect();
                words.sort();
                words.dedup();
                Folded {
                    number: model_number(&e.id),
                    id: fold(&e.id),
                    name,
                    title,
                    authors,
                    journal,
                    synopsis: fold(&e.synopsis),
                    year: e.year,
                    words,
                }
            })
            .collect();
        SearchIndex { items }
    }

    /// The indices of the matching entries, best first. An empty query
    /// matches every model, in id order.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let terms = parse_query(query);
        if terms.is_empty() {
            return (0..self.items.len()).collect();
        }
        let mut hits: Vec<(u32, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| score(item, &terms).map(|s| (s, i)))
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        hits.into_iter().map(|(_, i)| i).collect()
    }
}

/// The score of a model for every term, or `None` if a term rules it out.
fn score(item: &Folded, terms: &[Term]) -> Option<u32> {
    let mut total = 0;
    for term in terms {
        match term {
            Term::Year { from, to } => match item.year {
                Some(y) if (*from..=*to).contains(&y) => total += 1,
                _ => return None,
            },
            Term::Text { field, text, negated } => {
                let s = text_score(item, field, text);
                match (s, negated) {
                    (0, false) | (1.., true) => return None,
                    _ => total += s,
                }
            }
        }
    }
    Some(total)
}

fn text_score(item: &Folded, field: &Field, text: &str) -> u32 {
    let in_field = |hay: &str, start: u32, inside: u32| {
        if starts_word(hay, text) {
            start
        } else if hay.contains(text) {
            inside
        } else {
            0
        }
    };
    let id_score = || {
        let number = text
            .strip_prefix("biomd")
            .unwrap_or(text)
            .parse::<u64>()
            .ok();
        if number.is_some() && number == item.number {
            1000
        } else if text.starts_with("biomd") && item.id.contains(text) {
            500
        } else {
            0
        }
    };
    match field {
        Field::Id => id_score(),
        Field::Name => in_field(&item.name, 12, 8),
        Field::Title => in_field(&item.title, 6, 4),
        Field::Author => in_field(&item.authors, 6, 3),
        Field::Journal => in_field(&item.journal, 3, 2),
        Field::Any => {
            let year = match text.parse::<i32>() {
                Ok(y) if Some(y) == item.year => 2,
                _ => 0,
            };
            let exact = id_score()
                + in_field(&item.name, 12, 8)
                + in_field(&item.title, 6, 4)
                + in_field(&item.authors, 6, 3)
                + in_field(&item.journal, 3, 2)
                + u32::from(item.synopsis.contains(text))
                + year;
            if exact > 0 {
                exact
            } else {
                u32::from(near_word(&item.words, text))
            }
        }
    }
}

/// Whether `needle` occurs in `hay` at the start of a word.
fn starts_word(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(i, _)| {
        hay[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric())
    })
}

/// Whether some word is within one typo of `text` (two for long words).
/// Short words are never matched this way: they would match too much.
fn near_word(words: &[String], text: &str) -> bool {
    let len = text.chars().count();
    if len < 5 || !text.chars().all(char::is_alphabetic) {
        return false;
    }
    let limit = if len >= 8 { 2 } else { 1 };
    words.iter().any(|w| {
        let wl = w.chars().count();
        wl.abs_diff(len) <= limit && edit_distance(w, text, limit) <= limit
    })
}

/// Levenshtein distance, giving up (returning `limit + 1`) once it exceeds `limit`.
fn edit_distance(a: &str, b: &str, limit: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        let mut row_min = cur[0];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            row_min = row_min.min(cur[j]);
        }
        if row_min > limit {
            return limit + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The number in a model id: 52 for BIOMD0000000052.
fn model_number(id: &str) -> Option<u64> {
    let digits: String = id.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn parse_query(query: &str) -> Vec<Term> {
    split_query(query)
        .into_iter()
        .filter_map(|(raw, quoted)| {
            let (negated, raw) = match raw.strip_prefix('-') {
                Some(rest) if !quoted && !rest.is_empty() => (true, rest.to_string()),
                _ => (false, raw),
            };
            let (field, value) = match (quoted, raw.split_once(':')) {
                (false, Some((f, v))) => match f.to_lowercase().as_str() {
                    "author" | "authors" | "a" => (Field::Author, v.to_string()),
                    "journal" | "j" => (Field::Journal, v.to_string()),
                    "title" | "t" => (Field::Title, v.to_string()),
                    "name" | "n" => (Field::Name, v.to_string()),
                    "id" => (Field::Id, v.to_string()),
                    "year" | "y" | "date" => return year_range(v),
                    _ => (Field::Any, raw.clone()),
                },
                _ => (Field::Any, raw.clone()),
            };
            let text = fold(value.trim_matches('"'));
            (!text.is_empty()).then_some(Term::Text { field, text, negated })
        })
        .collect()
}

/// `2005`, `2000-2010`, `2000-`, `-2010`, `>2015`, `<1995`.
fn year_range(v: &str) -> Option<Term> {
    let v = v.trim();
    let year = |s: &str| s.trim().parse::<i32>().ok();
    let (from, to) = if let Some(s) = v.strip_prefix(">=") {
        (year(s)?, i32::MAX)
    } else if let Some(s) = v.strip_prefix('>') {
        (year(s)? + 1, i32::MAX)
    } else if let Some(s) = v.strip_prefix("<=") {
        (i32::MIN, year(s)?)
    } else if let Some(s) = v.strip_prefix('<') {
        (i32::MIN, year(s)? - 1)
    } else if let Some((a, b)) = v.split_once('-') {
        (year(a).unwrap_or(i32::MIN), year(b).unwrap_or(i32::MAX))
    } else {
        let y = year(v)?;
        (y, y)
    };
    Some(Term::Year { from, to })
}

/// Splits on whitespace, keeping "quoted phrases" (also after `field:`)
/// together. Returns each piece and whether it was a bare quoted phrase.
fn split_query(query: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut bare_quote = false;
    for c in query.chars() {
        match c {
            '"' => {
                if !in_quotes && cur.is_empty() {
                    bare_quote = true;
                }
                in_quotes = !in_quotes;
                cur.push(c);
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), bare_quote));
                }
                bare_quote = false;
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push((cur, bare_quote));
    }
    out.into_iter()
        .map(|(s, q)| if q { (s.trim_matches('"').to_string(), true) } else { (s, false) })
        .collect()
}

/// Lower case, with accents removed (letters decomposed, marks dropped).
pub fn fold(s: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, authors: &[&str], journal: &str, year: i32) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: name.into(),
            title: String::new(),
            authors: authors.iter().map(|s| s.to_string()).collect(),
            journal: journal.into(),
            date: year.to_string(),
            year: Some(year),
            synopsis: String::new(),
            citation: None,
            url: None,
        }
    }

    fn index() -> (Vec<ModelEntry>, SearchIndex) {
        let entries = vec![
            entry("BIOMD0000000005", "Tyson1991 - Cell Cycle", &["J J Tyson"], "PNAS", 1991),
            entry("BIOMD0000000052", "Teusink2000 - Glycolysis", &["B Teusink"], "Eur J Biochem", 2000),
            entry("BIOMD0000000172", "Pritchard2002 glycolysis", &["L Pritchard"], "Biophys J", 2002),
            entry("BIOMD0000000500", "Muller calcium model", &["S M\u{fc}ller"], "Biophys J", 2010),
        ];
        let index = SearchIndex::new(&entries);
        (entries, index)
    }

    fn ids(q: &str) -> Vec<String> {
        let (entries, index) = index();
        index.search(q).into_iter().map(|i| entries[i].id[11..].to_string()).collect()
    }

    #[test]
    fn number_finds_the_model_first() {
        assert_eq!(ids("52")[0], "0052");
        assert_eq!(ids("BIOMD52"), vec!["0052"]);
    }

    #[test]
    fn words_fields_years_and_exclusions() {
        assert_eq!(ids("glycolysis"), vec!["0052", "0172"]);
        assert_eq!(ids("glycolysis -teusink"), vec!["0172"]);
        assert_eq!(ids("author:tyson"), vec!["0005"]);
        assert_eq!(ids("j:biophys year:>2005"), vec!["0500"]);
        assert_eq!(ids("year:1990-2001"), vec!["0005", "0052"]);
        assert_eq!(ids("\"cell cycle\""), vec!["0005"]);
    }

    #[test]
    fn accents_and_typos() {
        assert_eq!(ids("muller"), vec!["0500"]);
        assert_eq!(ids("glycolsis"), vec!["0052", "0172"]);
    }

    #[test]
    fn empty_query_lists_everything() {
        assert_eq!(ids("  ").len(), 4);
    }
}
