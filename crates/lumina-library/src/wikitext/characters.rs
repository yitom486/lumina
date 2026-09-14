use crate::model::WikiCharacter;
use crate::wikitext::plain::{strip_refs, wikitext_to_plain};
use crate::wikitext::sections::{section_content, subsection_content};
use crate::wikitext::table::{header_column_map, parse_tables, pick_column};

/// Header aliases shared by the English and Chinese cast tables. Traditional
/// forms matter: zh articles use 演員/介紹 while the simplified aliases
/// below would never `contains`-match them.
const CHARACTER_COLUMN_ALIASES: &[&str] = &["character", "role", "name", "角色", "人物"];
const ACTOR_COLUMN_ALIASES: &[&str] =
    &["actor", "portrayedby", "castmember", "演员", "演員", "饰演"];
const BIO_COLUMN_ALIASES: &[&str] = &["description", "intro", "biography", "介绍", "介紹", "简介"];

/// Cast triples from a Chinese article's 演員陣容 section. Every table in
/// the section is read (main + supporting groups). The English
/// `WikiCharacter` fields carry the Chinese names (`name` = character,
/// `actor` = actor) so no new model type is needed downstream.
pub fn zh_cast_from_wikitext(wikitext: &str) -> Vec<WikiCharacter> {
    const ZH_CAST_SECTIONS: &[&str] = &["演員陣容", "演员阵容", "登場人物", "登场人物"];
    const MAX_ZH_CAST: usize = 40;

    let Some(section) = section_content(wikitext, ZH_CAST_SECTIONS) else {
        return Vec::new();
    };
    // Citations only add noise (and stray pipes) to table cells.
    let section = strip_refs(&section);
    let mut members = Vec::new();
    for table in parse_tables(&section) {
        let cleaned: Vec<Vec<String>> = table
            .iter()
            .map(|row| row.iter().map(|cell| strip_table_cell_attr(cell)).collect())
            .collect();
        let Some((header_index, header_map)) = find_cast_header(&cleaned) else {
            continue;
        };
        let character_col = pick_column(&header_map, CHARACTER_COLUMN_ALIASES);
        let actor_col = pick_column(&header_map, ACTOR_COLUMN_ALIASES);
        let bio_col = pick_column(&header_map, BIO_COLUMN_ALIASES);
        let (Some(character_col), Some(actor_col)) = (character_col, actor_col) else {
            continue;
        };
        for row in cleaned.iter().skip(header_index + 1) {
            let name = cell_at(row, character_col);
            let actor = cell_at(row, actor_col);
            if name.is_empty() || actor.is_empty() {
                continue;
            }
            let bio = bio_col.and_then(|index| {
                let text = cell_at(row, index);
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            });
            members.push(WikiCharacter { name, actor, bio });
            if members.len() >= MAX_ZH_CAST {
                return members;
            }
        }
    }
    members
}

/// Drop leading `align=left|` / `width=15%|` style cell attributes. The guard
/// keeps real content: piped links (`[[盧正義 (演員)|盧正義]]`) and templates
/// (`{{lk|…}}`) never match the attribute shape.
fn strip_table_cell_attr(cell: &str) -> String {
    let trimmed = cell.trim();
    if let Some((attr, rest)) = trimmed.split_once('|') {
        let attr = attr.trim().trim_start_matches(['!', '|']).trim();
        let rest = rest.trim();
        if !rest.is_empty()
            && attr.contains('=')
            && !attr
                .chars()
                .any(|c| c == ' ' || ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

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

    let character_col = pick_column(&header_map, CHARACTER_COLUMN_ALIASES);
    let actor_col = pick_column(&header_map, ACTOR_COLUMN_ALIASES);
    let bio_col = pick_column(&header_map, BIO_COLUMN_ALIASES);

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
        let has_character = pick_column(&header_map, CHARACTER_COLUMN_ALIASES);
        let has_actor = pick_column(&header_map, ACTOR_COLUMN_ALIASES);
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

    const ZH_CAST_FIXTURE: &str = include_str!("fixtures/zh_cast_snippet.txt");

    #[test]
    fn parses_zh_cast_tables_across_sections() {
        let members = zh_cast_from_wikitext(ZH_CAST_FIXTURE);
        assert_eq!(members.len(), 6);
        assert_eq!(members[0].name, "崔雄");
        assert_eq!(members[0].actor, "崔宇植");
        assert!(members[0]
            .bio
            .as_deref()
            .unwrap_or("")
            .contains("建築插畫家"));
        // Piped link keeps the display label, not the target.
        assert_eq!(members[2].actor, "盧正義");
        assert_eq!(members[2].name, "NJ");
        // Display-link template keeps the local label.
        assert_eq!(members[3].actor, "鄭強熙");
        assert_eq!(members[3].name, "昌植");
        // Plain-text actor without any link.
        assert_eq!(members[4].actor, "車承燁");
        // `n/a` role falls back to its display text.
        assert_eq!(members[5].actor, "金柱憲");
        assert_eq!(members[5].name, "僅聲音出演");
        for member in &members {
            assert!(!member.name.contains("align="));
            assert!(!member.actor.contains("童年"));
            let bio = member.bio.as_deref().unwrap_or("");
            assert!(!bio.contains("align="));
        }
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
