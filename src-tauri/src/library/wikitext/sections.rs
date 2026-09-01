//! Locate section bodies in raw wikitext by heading title.

pub fn section_content(wikitext: &str, title_needles: &[&str]) -> Option<String> {
    section_content_at_level(wikitext, title_needles, 2)
}

/// Extract a subsection body (e.g. `=== Main ===`) from an already isolated section.
pub fn subsection_content(input: &str, title_needles: &[&str]) -> Option<String> {
    section_content_at_level(input, title_needles, 3)
}

fn section_content_at_level(
    wikitext: &str,
    title_needles: &[&str],
    heading_level: usize,
) -> Option<String> {
    let lines: Vec<&str> = wikitext.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        if let Some((level, title)) = parse_heading(lines[index]) {
            if level == heading_level && title_matches_any(&title, title_needles) {
                let mut body = Vec::new();
                index += 1;
                while index < lines.len() {
                    if let Some((next_level, _)) = parse_heading(lines[index]) {
                        if next_level <= level {
                            break;
                        }
                    }
                    body.push(lines[index]);
                    index += 1;
                }
                let text = body.join("\n").trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        index += 1;
    }
    None
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('=') {
        return None;
    }
    let level = trimmed.chars().take_while(|ch| *ch == '=').count();
    if level < 2 {
        return None;
    }
    let closing = "=".repeat(level);
    if !trimmed.ends_with(&closing) || trimmed.len() <= level * 2 {
        return None;
    }
    let title = trimmed[level..trimmed.len() - level].trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some((level, title))
}

fn title_matches_any(title: &str, needles: &[&str]) -> bool {
    let normalized = normalize_title(title);
    needles.iter().any(|needle| {
        let needle = normalize_title(needle);
        normalized == needle || normalized.contains(&needle)
    })
}

fn normalize_title(title: &str) -> String {
    title
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_cast_section_until_next_heading() {
        let text = r#"
== Plot ==
plot body

== Cast ==
{| class="wikitable"
|-
| A || B
|}
== External links ==
links
"#;
        let section = section_content(text, &["cast"]).expect("cast section");
        assert!(section.contains("{|"));
        assert!(!section.contains("External links"));
    }
}
