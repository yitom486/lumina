use crate::library::model::WikiCharacter;
use crate::library::wikitext::plain::wikitext_to_plain;
use crate::library::wikitext::sections::section_content;
use crate::library::wikitext::table::{header_column_map, parse_tables, pick_column};

const CAST_SECTIONS: &[&str] = &["cast", "main characters", "characters"];

pub fn characters_from_wikitext(wikitext: &str) -> Vec<WikiCharacter> {
    let Some(section) = section_content(wikitext, CAST_SECTIONS) else {
        return Vec::new();
    };
    characters_from_cast_section(&section)
}

pub fn characters_from_cast_section(section: &str) -> Vec<WikiCharacter> {
    let tables = parse_tables(section);
    let Some(table) = tables.first() else {
        return Vec::new();
    };
    let Some((header_index, header_map)) = find_cast_header(table) else {
        return Vec::new();
    };

    let character_col = pick_column(
        &header_map,
        &["character", "role", "name", "角色", "人物"],
    );
    let actor_col = pick_column(
        &header_map,
        &["actor", "portrayedby", "castmember", "演员", "饰演"],
    );
    let bio_col = pick_column(
        &header_map,
        &["description", "intro", "biography", "介绍", "简介"],
    );

    let (character_col, actor_col) = match (character_col, actor_col) {
        (Some(c), Some(a)) => (c, a),
        _ => return Vec::new(),
    };

    let mut characters = Vec::new();
    for row in table.iter().skip(header_index + 1) {
        let name = cell_at(row, character_col);
        let actor = cell_at(row, actor_col);
        if name.is_empty() || actor.is_empty() {
            continue;
        }
        let bio = bio_col
            .and_then(|index| {
                let text = wikitext_to_plain(&cell_at(row, index));
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            });
        characters.push(WikiCharacter { name, actor, bio });
    }
    characters
}

fn find_cast_header(table: &[Vec<String>]) -> Option<(usize, Vec<(String, usize)>)> {
    for (index, row) in table.iter().enumerate() {
        let header_map = header_column_map(row);
        let has_character = pick_column(
            &header_map,
            &["character", "role", "name", "角色", "人物"],
        );
        let has_actor = pick_column(
            &header_map,
            &["actor", "portrayedby", "castmember", "演员", "饰演"],
        );
        if has_character.is_some() && has_actor.is_some() {
            return Some((index, header_map));
        }
    }
    None
}

fn cell_at(row: &[String], index: usize) -> String {
    row.get(index)
        .map(|cell| wikitext_to_plain(cell))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAST_FIXTURE: &str = r#"
== Cast ==
{| class="wikitable plainrowheaders"
|-
! Character !! Actor !! Description
|-
| [[Choi Ung]] || [[Choi Woo-shik]] || A cartoonist who published a hit book in high school.
|-
| [[Na Bo-ra]] || [[Kim Da-mi]] || Ung's ex-girlfriend and a marketing employee.
|-
| [[Kim Ji-ung]] || [[Roh Jeong-eui]] || A documentary filmmaker and Bo-ra's best friend.
|-
| [[Gu Eun-ho]] || [[Kim Sung-cheol]] || A successful architect and Ung's friend.
|}
"#;

    #[test]
    fn parses_main_cast_from_table() {
        let characters = characters_from_wikitext(CAST_FIXTURE);
        assert_eq!(characters.len(), 4);
        assert_eq!(characters[0].name, "Choi Ung");
        assert_eq!(characters[0].actor, "Choi Woo-shik");
        assert!(characters[0]
            .bio
            .as_deref()
            .unwrap_or("")
            .contains("cartoonist"));
    }
}
