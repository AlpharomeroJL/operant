//! Minimal, purpose-built HTML scanning for the fixture webapp
//! (`contracts/fixtures/webapp/{index,drift}.html`). No general HTML/DOM
//! parser dependency: regex over the small, well-formed markup these two
//! checked-in fixtures ship is enough, matching this crate's existing
//! precedent for fixture-scale parsing (`super::email::parse`'s
//! hand-rolled RFC 5322 reader, `super::ocr`'s hand-rolled PNG/PDF
//! readers). Nesting, attribute escaping beyond the five named HTML
//! entities, and malformed-HTML recovery are explicitly out of scope.

use regex::Regex;

/// One tracked `<input>`: an id plus its accessible name (`aria-label`,
/// falling back to an associated `<label for="...">`) and HTML `type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputField {
    pub id: String,
    pub label: String,
    pub input_type: String,
}

/// A named, id-bearing control: the submit button or the results list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedControl {
    pub id: String,
    pub label: String,
}

/// The handful of accessibility-relevant fields C5's fixture shape needs,
/// extracted from one HTML document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageFields {
    pub title: String,
    pub inputs: Vec<InputField>,
    pub button: Option<NamedControl>,
    pub list: Option<NamedControl>,
}

/// Extract `<title>`, every non-`date`/`hidden` `<input>` (paired with its
/// accessible name), the first `<button>` (paired with its inner text),
/// and the first `aria-label`led `<ul>`. Driven entirely by `html`'s
/// actual text -- nothing here is keyed off a filename -- so
/// `webapp/drift.html`'s renamed button changes the extracted `button`
/// exactly the way a real DOM read would.
pub fn extract_page(html: &str) -> PageFields {
    let mut fields = PageFields::default();

    if let Some(cap) = title_re().captures(html) {
        fields.title = decode_entities(cap[1].trim());
    }

    for cap in input_re().captures_iter(html) {
        let tag = &cap[0];
        let input_type = attr(tag, "type").unwrap_or_else(|| "text".to_string());
        if input_type == "date" || input_type == "hidden" {
            continue;
        }
        let Some(id) = attr(tag, "id") else {
            continue;
        };
        let label = attr(tag, "aria-label")
            .or_else(|| label_for(html, &id))
            .unwrap_or_else(|| id.clone());
        fields.inputs.push(InputField {
            id,
            label: decode_entities(&label),
            input_type,
        });
    }

    if let Some(cap) = button_re().captures(html) {
        if let Some(id) = attr(&cap[1], "id") {
            fields.button = Some(NamedControl {
                id,
                label: decode_entities(cap[2].trim()),
            });
        }
    }

    if let Some(cap) = list_re().captures(html) {
        let tag = &cap[0];
        if let (Some(id), Some(label)) = (attr(tag, "id"), attr(tag, "aria-label")) {
            fields.list = Some(NamedControl {
                id,
                label: decode_entities(&label),
            });
        }
    }

    fields
}

/// Read one `name="value"` attribute out of a single already-matched tag
/// string (never the whole document, so there is no risk of matching an
/// attribute that belongs to a different element).
fn attr(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"{}="([^"]*)""#, regex::escape(name))).ok()?;
    re.captures(tag).map(|c| c[1].to_string())
}

fn label_for(html: &str, id: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"(?is)<label\s+for="{}"[^>]*>([^<]*)</label>"#,
        regex::escape(id)
    ))
    .ok()?;
    re.captures(html).map(|c| c[1].trim().to_string())
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn title_re() -> Regex {
    Regex::new(r"(?is)<title>(.*?)</title>").unwrap()
}
fn input_re() -> Regex {
    Regex::new(r"(?i)<input\b[^>]*>").unwrap()
}
fn button_re() -> Regex {
    Regex::new(r"(?is)(<button\b[^>]*>)(.*?)</button>").unwrap()
}
fn list_re() -> Regex {
    Regex::new(r"(?i)<ul\b[^>]*>").unwrap()
}

#[cfg(test)]
mod tests {
    use super::super::fixture::FixtureBrowser;
    use super::*;

    fn read_fixture(name: &str) -> String {
        std::fs::read_to_string(FixtureBrowser::fixtures_dir().join(name))
            .unwrap_or_else(|e| panic!("reading fixture {name}: {e}"))
    }

    #[test]
    fn extracts_title_inputs_button_and_list_from_the_index_fixture() {
        let fields = extract_page(&read_fixture("index.html"));
        assert_eq!(fields.title, "Operant Fixture Invoices");

        assert_eq!(fields.inputs.len(), 2, "date input must be excluded");
        assert_eq!(fields.inputs[0].id, "customer");
        assert_eq!(fields.inputs[0].label, "Customer");
        assert_eq!(fields.inputs[0].input_type, "text");
        assert_eq!(fields.inputs[1].id, "amount");
        assert_eq!(fields.inputs[1].label, "Amount");

        let button = fields.button.expect("button present");
        assert_eq!(button.id, "save-btn");
        assert_eq!(button.label, "Save invoice");

        let list = fields.list.expect("list present");
        assert_eq!(list.id, "invoice-list");
        assert_eq!(list.label, "Saved invoices");
    }

    #[test]
    fn drift_variant_extracts_the_renamed_button_and_nothing_else_changes() {
        let base = extract_page(&read_fixture("index.html"));
        let drift = extract_page(&read_fixture("drift.html"));

        assert_eq!(drift.title, base.title);
        assert_eq!(drift.inputs, base.inputs);
        assert_eq!(drift.list, base.list);

        let button = drift.button.expect("button present in the drift variant");
        assert_eq!(button.id, "store-btn");
        assert_eq!(button.label, "Store invoice");
        assert_ne!(Some(button), base.button);
    }
}
