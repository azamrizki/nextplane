//! HTML of a page's Yjs document, built on the server so the snapshot used for reading,
//! search, versions and agents always matches what was saved. The document is Tiptap's
//! ProseMirror tree as y-prosemirror stores it: elements named after node types (with node
//! attributes), text with marks as formatting attributes.

use yrs::types::text::YChange;
use yrs::types::xml::{Xml, XmlFragment, XmlOut};
use yrs::{Any, Doc, Out, ReadTxn, Text, Transact};

/// The field Tiptap's Collaboration extension uses.
pub const FIELD: &str = "default";

pub fn doc_html(doc: &Doc) -> String {
    let txn = doc.transact();
    // A document nobody has typed in has no fragment yet (and creating one needs a write).
    let Some(frag) = txn.get_xml_fragment(FIELD) else {
        return String::new();
    };
    let mut out = String::new();
    for c in frag.children(&txn) {
        node(&txn, c, &mut out, 0);
    }
    crate::richtext::sanitize_html(&out)
}

fn attr_str<T: ReadTxn, X: Xml>(txn: &T, x: &X, name: &str) -> Option<String> {
    match x.get_attribute(txn, name)? {
        Out::Any(a) => any_str(&a),
        other => Some(other.to_string(txn)),
    }
}

fn any_str(a: &Any) -> Option<String> {
    match a {
        Any::Null | Any::Undefined => None,
        Any::String(s) => Some(s.to_string()),
        Any::Number(n) => Some(if n.fract() == 0.0 {
            format!("{}", *n as i64)
        } else {
            n.to_string()
        }),
        Any::BigInt(n) => Some(n.to_string()),
        Any::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn node<T: ReadTxn>(txn: &T, n: XmlOut, out: &mut String, depth: usize) {
    if depth > 64 {
        return;
    }
    match n {
        XmlOut::Text(t) => text(txn, &t, out),
        XmlOut::Fragment(f) => {
            for c in f.children(txn) {
                node(txn, c, out, depth + 1);
            }
        }
        XmlOut::Element(e) => {
            let kids = |out: &mut String| {
                for c in e.children(txn) {
                    node(txn, c, out, depth + 1);
                }
            };
            let wrap = |out: &mut String, open: &str, close: &str| {
                out.push_str(open);
                kids(out);
                out.push_str(close);
            };
            match e.tag().as_ref() {
                "paragraph" => wrap(out, "<p>", "</p>"),
                "heading" => {
                    let l = attr_str(txn, &e, "level")
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(1)
                        .clamp(1, 4);
                    wrap(out, &format!("<h{l}>"), &format!("</h{l}>"));
                }
                "blockquote" => wrap(out, "<blockquote>", "</blockquote>"),
                "bulletList" => wrap(out, "<ul>", "</ul>"),
                "orderedList" => match attr_str(txn, &e, "start").filter(|s| s != "1") {
                    Some(s) => {
                        let mut open = String::from("<ol start=\"");
                        esc(&s, &mut open);
                        open.push_str("\">");
                        wrap(out, &open, "</ol>");
                    }
                    None => wrap(out, "<ol>", "</ol>"),
                },
                "listItem" => wrap(out, "<li>", "</li>"),
                "taskList" => wrap(out, "<ul data-type=\"taskList\">", "</ul>"),
                "taskItem" => {
                    let checked = attr_str(txn, &e, "checked").as_deref() == Some("true");
                    out.push_str(&format!(
                        "<li data-type=\"taskItem\" data-checked=\"{checked}\"><label><input type=\"checkbox\"{}></label><div>",
                        if checked { " checked=\"checked\"" } else { "" }
                    ));
                    kids(out);
                    out.push_str("</div></li>");
                }
                "codeBlock" => {
                    match attr_str(txn, &e, "language") {
                        Some(lang) => {
                            out.push_str("<pre><code class=\"language-");
                            esc(&lang, out);
                            out.push_str("\">");
                        }
                        None => out.push_str("<pre><code>"),
                    }
                    kids(out);
                    out.push_str("</code></pre>");
                }
                "horizontalRule" => out.push_str("<hr>"),
                "hardBreak" => out.push_str("<br>"),
                "image" => {
                    if let Some(src) = attr_str(txn, &e, "src") {
                        out.push_str("<img src=\"");
                        esc(&src, out);
                        out.push('"');
                        if let Some(alt) = attr_str(txn, &e, "alt") {
                            out.push_str(" alt=\"");
                            esc(&alt, out);
                            out.push('"');
                        }
                        out.push('>');
                    }
                }
                "mention" => {
                    let label = attr_str(txn, &e, "label").unwrap_or_default();
                    out.push_str("<span data-type=\"mention\">@");
                    esc(&label, out);
                    out.push_str("</span>");
                }
                // Unknown nodes keep their content.
                _ => kids(out),
            }
        }
    }
}

/// Marks in a fixed nesting order (outermost first) with their tags.
const MARKS: &[(&str, &str, &str)] = &[
    ("bold", "<strong>", "</strong>"),
    ("italic", "<em>", "</em>"),
    ("underline", "<u>", "</u>"),
    ("strike", "<s>", "</s>"),
    ("highlight", "<mark>", "</mark>"),
    ("code", "<code>", "</code>"),
];

fn text<T: ReadTxn>(txn: &T, t: &yrs::XmlTextRef, out: &mut String) {
    for d in t.diff(txn, YChange::identity) {
        let Out::Any(Any::String(s)) = &d.insert else {
            continue;
        };
        // y-prosemirror keys overlapping marks as `name--hash`.
        let names: Vec<(&str, &Any)> = d
            .attributes
            .as_deref()
            .map(|a| {
                a.iter()
                    .map(|(k, v)| (k.split("--").next().unwrap_or(k), v))
                    .collect()
            })
            .unwrap_or_default();
        let has = |m: &str| names.iter().any(|(k, _)| *k == m);
        let href = names
            .iter()
            .find(|(k, _)| *k == "link")
            .and_then(|(_, v)| match v {
                Any::Map(m) => m.get("href").and_then(any_str),
                _ => None,
            });
        if let Some(h) = &href {
            out.push_str("<a href=\"");
            esc(h, out);
            out.push_str("\">");
        }
        for (m, open, _) in MARKS {
            if has(m) {
                out.push_str(open);
            }
        }
        esc(s, out);
        for (m, _, close) in MARKS.iter().rev() {
            if has(m) {
                out.push_str(close);
            }
        }
        if href.is_some() {
            out.push_str("</a>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use yrs::types::xml::{XmlElementPrelim, XmlTextPrelim};
    use yrs::{XmlElementRef, XmlTextRef};

    #[test]
    fn tiptap_tree_to_html() {
        let doc = Doc::new();
        let frag = doc.get_or_insert_xml_fragment(FIELD);
        {
            let mut tx = doc.transact_mut();
            let h: XmlElementRef = frag.push_back(&mut tx, XmlElementPrelim::empty("heading"));
            h.insert_attribute(&mut tx, "level", Any::Number(2.0));
            let ht: XmlTextRef = h.push_back(&mut tx, XmlTextPrelim::new("Plan <v2>"));
            let _ = ht;
            let p: XmlElementRef = frag.push_back(&mut tx, XmlElementPrelim::empty("paragraph"));
            let t: XmlTextRef = p.push_back(&mut tx, XmlTextPrelim::new(""));
            t.insert(&mut tx, 0, "Ship ");
            let bold: yrs::types::Attrs =
                HashMap::from([(Arc::from("bold"), Any::Map(Arc::new(HashMap::new())))]);
            t.insert_with_attributes(&mut tx, 5, "now", bold);
            let link: yrs::types::Attrs = HashMap::from([(
                Arc::from("link"),
                Any::Map(Arc::new(HashMap::from([(
                    "href".to_string(),
                    Any::String("https://example.com".into()),
                )]))),
            )]);
            t.insert_with_attributes(&mut tx, 8, " docs", link);
            let list: XmlElementRef = frag.push_back(&mut tx, XmlElementPrelim::empty("taskList"));
            let item: XmlElementRef = list.push_back(&mut tx, XmlElementPrelim::empty("taskItem"));
            item.insert_attribute(&mut tx, "checked", Any::Bool(true));
            let ip: XmlElementRef = item.push_back(&mut tx, XmlElementPrelim::empty("paragraph"));
            ip.push_back(&mut tx, XmlTextPrelim::new("Done"));
            let bad: XmlElementRef = frag.push_back(&mut tx, XmlElementPrelim::empty("paragraph"));
            bad.push_back(&mut tx, XmlTextPrelim::new("<script>alert(1)</script>"));
        }
        let html = doc_html(&doc);
        assert!(html.starts_with("<h2>Plan &lt;v2&gt;</h2><p>Ship <strong>now</strong><a href=\"https://example.com\""), "{html}");
        assert!(html.contains("data-checked=\"true\""), "{html}");
        assert!(html.contains("Done"));
        assert!(!html.contains("<script>"), "{html}");
    }
}
