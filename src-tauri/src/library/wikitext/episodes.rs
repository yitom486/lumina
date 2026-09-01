use crate::library::model::WikiEpisodeSummary;
use crate::library::wikitext::plain::wikitext_to_plain;
use crate::library::wikitext::sections::section_content;

const EPISODE_SECTIONS: &[&str] = &[
    "summary of each episode",
    "episode summaries",
    "episodes",
    "分集",
    "剧情",
];

pub fn episodes_from_wikitext(wikitext: &str) -> Vec<WikiEpisodeSummary> {
    let Some(section) = section_content(wikitext, EPISODE_SECTIONS) else {
        return Vec::new();
    };
    episodes_from_summary_section(&section)
}

pub fn episodes_from_summary_section(section: &str) -> Vec<WikiEpisodeSummary> {
    let mut episodes = parse_definition_list_episodes(section);
    if episodes.is_empty() {
        episodes = parse_heading_episodes(section);
    }
    episodes
}

fn parse_definition_list_episodes(section: &str) -> Vec<WikiEpisodeSummary> {
    let mut episodes = Vec::new();
    let lines: Vec<&str> = section.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if let Some((episode, title)) = parse_def_list_marker(trimmed) {
            index += 1;
            let mut plot_lines = Vec::new();
            while index < lines.len() {
                let next = lines[index].trim();
                if parse_def_list_marker(next).is_some() || parse_subheading(next).is_some() {
                    break;
                }
                if let Some(text) = next.strip_prefix(':') {
                    plot_lines.push(text.trim());
                } else if !next.is_empty() && !next.starts_with('{') {
                    break;
                }
                index += 1;
            }
            let plot = wikitext_to_plain(&plot_lines.join(" "));
            if !plot.is_empty() {
                episodes.push(WikiEpisodeSummary {
                    season: 1,
                    episode,
                    title,
                    plot,
                });
            }
            continue;
        }
        index += 1;
    }
    episodes
}

fn parse_heading_episodes(section: &str) -> Vec<WikiEpisodeSummary> {
    let mut episodes = Vec::new();
    let lines: Vec<&str> = section.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        if let Some((episode, title)) = parse_subheading(lines[index]) {
            index += 1;
            let mut body = Vec::new();
            while index < lines.len() {
                if parse_subheading(lines[index]).is_some() || parse_def_list_marker(lines[index].trim()).is_some() {
                    break;
                }
                let line = lines[index].trim();
                if !line.is_empty() && !line.starts_with('{') {
                    body.push(line);
                }
                index += 1;
            }
            let plot = wikitext_to_plain(&body.join(" "));
            if !plot.is_empty() {
                episodes.push(WikiEpisodeSummary {
                    season: 1,
                    episode,
                    title,
                    plot,
                });
            }
            continue;
        }
        index += 1;
    }
    episodes
}

fn parse_def_list_marker(line: &str) -> Option<(u32, Option<String>)> {
    let trimmed = line.trim();
    if !trimmed.starts_with(';') {
        return None;
    }
    let body = trimmed.trim_start_matches(';').trim();
    parse_episode_label(body)
}

fn parse_subheading(line: &str) -> Option<(u32, Option<String>)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("===") {
        return None;
    }
    let level = trimmed.chars().take_while(|ch| *ch == '=').count();
    if level < 3 {
        return None;
    }
    let closing = "=".repeat(level);
    if !trimmed.ends_with(&closing) {
        return None;
    }
    let title = trimmed[level..trimmed.len() - level].trim();
    parse_episode_label(title)
}

fn parse_episode_label(label: &str) -> Option<(u32, Option<String>)> {
    if let Some(rest) = label.strip_prefix("Episode ") {
        return parse_number_and_title(rest);
    }
    parse_number_and_title(label)
}

fn parse_number_and_title(label: &str) -> Option<(u32, Option<String>)> {
    let trimmed = label.trim();
    if let Some((number, title)) = trimmed.split_once('"') {
        let episode = number.trim().trim_end_matches(':').parse().ok()?;
        let title = title.strip_suffix('"').map(str::trim).filter(|text| !text.is_empty());
        return Some((episode, title.map(str::to_string)));
    }
    let episode = trimmed
        .split_whitespace()
        .next()?
        .trim_end_matches(':')
        .parse()
        .ok()?;
    Some((episode, None))
}

pub fn episode_plot_for(
    episodes: &[WikiEpisodeSummary],
    season: u32,
    episode: u32,
) -> Option<String> {
    episodes
        .iter()
        .find(|entry| entry.season == season && entry.episode == episode)
        .map(|entry| entry.plot.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPISODE_FIXTURE: &str = r#"
== Summary of each episode ==
; 1
: Ung and Bo-ra meet again after ten years.
; 2 "You're My Most Precious Gift"
: Bo-ra asks Ung to join a documentary project.
; 3
: The crew begins filming around Ung's neighborhood.
"#;

    #[test]
    fn parses_definition_list_episodes() {
        let episodes = episodes_from_wikitext(EPISODE_FIXTURE);
        assert_eq!(episodes.len(), 3);
        assert_eq!(episodes[0].episode, 1);
        assert_eq!(
            episodes[1].title.as_deref(),
            Some("You're My Most Precious Gift")
        );
        assert!(episodes[2].plot.contains("filming"));
    }

    #[test]
    fn finds_plot_by_season_and_episode() {
        let episodes = episodes_from_wikitext(EPISODE_FIXTURE);
        assert_eq!(
            episode_plot_for(&episodes, 1, 2).as_deref(),
            Some("Bo-ra asks Ung to join a documentary project.")
        );
    }
}
