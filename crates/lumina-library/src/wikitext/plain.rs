//! Strip common wikitext markup into plain text for summaries and bios.

pub fn wikitext_to_plain(input: &str) -> String {
    let without_refs = strip_refs(input);
    let without_templates = strip_templates(&without_refs);
    let without_comments = strip_html_comments(&without_templates);
    let linked = strip_links(&without_comments);
    let unformatted = strip_formatting(&linked);
    collapse_whitespace(&unformatted)
}

fn strip_html_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 4..];
        if let Some(end) = rest.find("-->") {
            rest = &rest[end + 3..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn strip_refs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<ref") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let close = rest.find("</ref>").map(|index| index + 6);
        let self_close = rest.find("/>").map(|index| index + 2);
        match (close, self_close) {
            (Some(c), Some(s)) if s < c => rest = &rest[s..],
            (Some(c), _) => rest = &rest[c..],
            (_, Some(s)) => rest = &rest[s..],
            _ => {
                out.push_str(rest);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_templates(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next();
            let mut body = String::new();
            if consume_balanced_body(&mut chars, &mut body) {
                out.push_str(&template_plain_fallback(&body));
                continue;
            }
            out.push('{');
            out.push('{');
            continue;
        }
        out.push(ch);
    }
    out
}

fn consume_balanced_body<I>(chars: &mut std::iter::Peekable<I>, body: &mut String) -> bool
where
    I: Iterator<Item = char>,
{
    let mut depth = 2;
    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next();
            depth += 2;
            body.push('{');
            body.push('{');
            continue;
        }
        if ch == '}' && chars.peek() == Some(&'}') {
            chars.next();
            depth -= 2;
            if depth == 0 {
                return true;
            }
            body.push('}');
            body.push('}');
            continue;
        }
        body.push(ch);
    }
    false
}

fn template_plain_fallback(body: &str) -> String {
    let mut parts = body.split('|');
    let name = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let params: Vec<&str> = parts
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    match name.as_str() {
        // Display-link templates: keep the local label, drop the foreign target.
        "lk" | "link-en" | "link-ko" | "link-ja" | "ill" => {
            params.first().unwrap_or(&"").to_string()
        }
        // Annotation wrappers contribute no readable text of their own
        // (e.g. `{{small|（童年：宋河賢）}}` must not leak into the actor name).
        "small" | "smalldiv" | "refn" | "efn" => String::new(),
        // Anything else keeps the previous behavior (last parameter).
        _ => params.last().unwrap_or(&"").to_string(),
    }
}

fn strip_links(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let inner = &rest[..end];
            let label = inner.split('|').next_back().unwrap_or(inner);
            out.push_str(label);
            rest = &rest[end + 2..];
        } else {
            out.push_str("[[");
            break;
        }
    }
    out.push_str(rest);
    out
}

fn strip_formatting(input: &str) -> String {
    input
        .replace("'''", "")
        .replace("''", "")
        .replace("<br />", " ")
        .replace("<br/>", " ")
        .replace("<br>", " ")
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_links_templates_and_refs() {
        let plain =
            wikitext_to_plain("[[Choi Ung|Ung]] meets {{nowrap|Na Bo-ra}} at school.<ref name=x/>");
        assert_eq!(plain, "Ung meets Na Bo-ra at school.");
    }

    #[test]
    fn display_link_templates_keep_the_local_label() {
        assert_eq!(wikitext_to_plain("{{lk|鄭強熙|정강희}}"), "鄭強熙");
        assert_eq!(
            wikitext_to_plain("{{link-en|李善熙|Lee Seung-hee}}"),
            "李善熙"
        );
        assert_eq!(wikitext_to_plain("{{n/a|僅聲音出演}}"), "僅聲音出演");
    }

    #[test]
    fn annotation_wrappers_contribute_no_text() {
        assert_eq!(
            wikitext_to_plain("[[崔宇植]]<br>{{small|（童年：宋河賢）}}"),
            "崔宇植"
        );
    }
}
