//! Parse wikitext `{| ... |}` tables into row/column matrices.

pub fn parse_tables(input: &str) -> Vec<Vec<Vec<String>>> {
    let mut tables = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find("{|") {
        let before = &rest[..start];
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("|}") {
            let table_body = &after_start[..end];
            tables.push(parse_table_body(table_body));
            rest = &after_start[end + 2..];
        } else {
            break;
        }
        let _ = before;
    }
    tables
}

fn parse_table_body(body: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for chunk in body.split("|-") {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cells = split_row_cells(trimmed);
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    rows
}

fn split_row_cells(row: &str) -> Vec<String> {
    let normalized = row.replace("!!", "||");
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = normalized.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '|' {
            if chars.peek() == Some(&'|') {
                chars.next();
                cells.push(clean_cell(&current));
                current.clear();
                continue;
            }
            if current.trim().is_empty() && cells.is_empty() {
                continue;
            }
        }
        current.push(ch);
    }
    if !current.trim().is_empty() || !cells.is_empty() {
        cells.push(clean_cell(&current));
    }
    cells.retain(|cell| !cell.is_empty());
    cells
}

fn clean_cell(input: &str) -> String {
    input.trim().trim_start_matches('!').trim().to_string()
}

pub fn header_column_map(header_row: &[String]) -> Vec<(String, usize)> {
    header_row
        .iter()
        .enumerate()
        .map(|(index, cell)| (normalize_header(cell), index))
        .collect()
}

pub fn pick_column(map: &[(String, usize)], aliases: &[&str]) -> Option<usize> {
    let normalized_aliases: Vec<String> =
        aliases.iter().map(|item| normalize_header(item)).collect();
    map.iter()
        .find(|(header, _)| {
            normalized_aliases
                .iter()
                .any(|alias| header.contains(alias.as_str()))
        })
        .map(|(_, index)| *index)
}

fn normalize_header(input: &str) -> String {
    input
        .trim()
        .trim_start_matches('!')
        .to_ascii_lowercase()
        .replace(' ', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_table_rows() {
        let tables = parse_tables(
            r#"{|
|-
! Character !! Actor !! Description
|-
| [[Ung]] || [[Choi Woo-shik]] || A cartoonist.
|}"#,
        );
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].len(), 2);
        assert_eq!(tables[0][0][0], "Character");
        assert_eq!(tables[0][1][1], "[[Choi Woo-shik]]");
    }
}
