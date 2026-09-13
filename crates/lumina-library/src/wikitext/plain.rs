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

fn strip_refs(input: &str) -> String {
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
    body.rsplit('|').next().unwrap_or("").trim().to_string()
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
}
