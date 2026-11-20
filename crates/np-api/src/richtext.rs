//! Rich text from the editor: the HTML is sanitized here with an allow-list before it is
//! stored; the Tiptap JSON is kept for editing and only checked to be JSON.

use np_core::work_item::BODY_MAX_BYTES;
use np_core::{Error, Result};

/// Tags and attributes the editor produces. Everything else is dropped.
pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .tags(
            [
                "p",
                "br",
                "strong",
                "b",
                "em",
                "i",
                "u",
                "s",
                "del",
                "code",
                "pre",
                "blockquote",
                "ul",
                "ol",
                "li",
                "h1",
                "h2",
                "h3",
                "h4",
                "a",
                "hr",
                "span",
                "label",
                "input",
                "div",
                "mark",
                "img",
            ]
            .into_iter()
            .collect(),
        )
        .add_tag_attributes("a", ["href", "title"])
        .add_tag_attributes("ol", ["start"])
        .add_tag_attributes("li", ["data-type", "data-checked"])
        .add_tag_attributes("ul", ["data-type"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("span", ["data-type", "data-id", "data-label", "class"])
        .add_tag_attributes("code", ["class"])
        .add_tag_attributes("img", ["src", "alt", "title"])
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(html)
        .to_string()
}

/// Validates an editor body: JSON (optional) and HTML, each within the size cap.
pub fn clean_body(json: Option<serde_json::Value>, html: &str) -> Result<(Option<String>, String)> {
    let json = json.filter(|j| !j.is_null()).map(|j| j.to_string());
    if json.as_ref().is_some_and(|j| j.len() > BODY_MAX_BYTES) || html.len() > BODY_MAX_BYTES {
        return Err(Error::bad_request("the text is too long"));
    }
    Ok((json, sanitize_html(html)))
}

/// Plain text of sanitized HTML, for search and previews.
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_handlers_are_removed() {
        let dirty = r#"<p onclick="x()">Hi <script>alert(1)</script><a href="javascript:alert(1)">x</a><a href="https://ok.test">ok</a><img src=x onerror=alert(1)></p>"#;
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("script"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("javascript"));
        assert!(!clean.contains("onerror"));
        assert!(clean.contains(r#"href="https://ok.test""#));
        assert!(clean.contains("noopener"));
    }

    #[test]
    fn task_lists_survive() {
        let html = r#"<ul data-type="taskList"><li data-type="taskItem" data-checked="true"><label><input type="checkbox" checked></label><div><p>done</p></div></li></ul>"#;
        let clean = sanitize_html(html);
        assert!(clean.contains(r#"data-checked="true""#));
        assert_eq!(plain_text(&clean), "done");
    }
}
