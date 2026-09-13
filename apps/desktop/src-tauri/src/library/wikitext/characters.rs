use crate::library::model::WikiCharacter;
use crate::library::wikitext::plain::wikitext_to_plain;
use crate::library::wikitext::sections::{section_content, subsection_content};
use crate::library::wikitext::table::{header_column_map, parse_tables, pick_column};

const CAST_SECTIONS: &[&str] = &[
    "cast and characters",
    "cast",
    "main characters",
    "characters",
];
const MAIN_CAST_SUBSECTIONS: &[&str] = &["main"];

pub fn characters_from_wikitext(wikitext: &str) -> Vec<WikiCharacter> {
    let Some(section) = section_content(wikitext, CAST_SECTIONS) else {
        return Vec::new();
    };
    let main_section = subsection_content(&section, MAIN_CAST_SUBSECTIONS).unwrap_or(section);
    let from_table = characters_from_cast_section(&main_section);
    if !from_table.is_empty() {
        return from_table;
    }
    characters_from_cast_list(&main_section)
}

pub fn characters_from_cast_list(section: &str) -> Vec<WikiCharacter> {
    let mut characters = Vec::new();
    let mut current: Option<(String, String, Vec<String>)> = None;

    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('*') && !trimmed.starts_with("**") {
            if let Some((name, actor, bio_parts)) = current.take() {
                push_character(&mut characters, name, actor, bio_parts);
            }
            if let Some((name, actor)) = parse_actor_role_line(trimmed) {
                current = Some((name, actor, Vec::new()));
            }
            continue;
        }
        if trimmed.starts_with(':') {
            if let Some((_, _, bio_parts)) = current.as_mut() {
                let text = wikitext_to_plain(trimmed.trim_start_matches(':').trim());
                if !text.is_empty() {
                    bio_parts.push(text);
                }
            }
        }
    }

    if let Some((name, actor, bio_parts)) = current {
        push_character(&mut characters, name, actor, bio_parts);
    }
    characters
}

fn parse_actor_role_line(line: &str) -> Option<(String, String)> {
    let line = line.trim().trim_start_matches('*').trim();
    let line = line.split('<').next().unwrap_or(line).trim();
    let (actor_part, role_part) = line.split_once(" as ")?;
    let actor = wikitext_to_plain(actor_part.trim());
    let name = wikitext_to_plain(role_part.trim());
    if actor.is_empty() || name.is_empty() {
        return None;
    }
    Some((name, actor))
}

fn push_character(
    characters: &mut Vec<WikiCharacter>,
    name: String,
    actor: String,
    bio_parts: Vec<String>,
) {
    let bio = if bio_parts.is_empty() {
        None
    } else {
        Some(bio_parts.join(" "))
    };
    characters.push(WikiCharacter { name, actor, bio });
}

pub fn characters_from_cast_section(section: &str) -> Vec<WikiCharacter> {
    let tables = parse_tables(section);
    let Some(table) = tables.first() else {
        return Vec::new();
    };
    let Some((header_index, header_map)) = find_cast_header(table) else {
        return Vec::new();
    };

    let character_col = pick_column(&header_map, &["character", "role", "name", "角色", "人物"]);
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
        let bio = bio_col.and_then(|index| {
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
        let has_character =
            pick_column(&header_map, &["character", "role", "name", "角色", "人物"]);
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

    #[test]
    fn parses_main_cast_from_definition_list() {
        const LIST_FIXTURE: &str = r#"
== Cast and characters ==
=== Main ===
* [[Koo Kyo-hwan]] as Hwang Dong-man<ref name="JTBC" />
: Among the eight members of the film club, he alone has not debuted.
* [[Go Youn-jung]] as Byeon Eun-ah<ref name="JTBC" />
** Han Si-a as young Eun-ah
: The production director of Choi Film.
=== Supporting ===
* [[Jeon Bae-soo]] as Park Young-soo
"#;
        let characters = characters_from_wikitext(LIST_FIXTURE);
        assert_eq!(characters.len(), 2);
        assert_eq!(characters[0].name, "Hwang Dong-man");
        assert_eq!(characters[0].actor, "Koo Kyo-hwan");
        assert!(characters[0]
            .bio
            .as_deref()
            .unwrap_or("")
            .contains("film club"));
        assert_eq!(characters[1].name, "Byeon Eun-ah");
        assert_eq!(characters[1].actor, "Go Youn-jung");
    }
}
