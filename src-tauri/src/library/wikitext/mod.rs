mod characters;
mod episodes;
mod plain;
mod sections;
mod table;

pub use characters::characters_from_wikitext;
pub use episodes::{episode_plot_for, episodes_from_wikitext};

use crate::library::model::{WikiCharacter, WikiEpisodeSummary};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuredWikiContent {
    pub characters: Vec<WikiCharacter>,
    pub episodes: Vec<WikiEpisodeSummary>,
}

pub fn parse_structured_content(wikitext: &str) -> StructuredWikiContent {
    StructuredWikiContent {
        characters: characters_from_wikitext(wikitext),
        episodes: episodes_from_wikitext(wikitext),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/our_beloved_summer_snippet.txt");

    #[test]
    fn integrated_fixture_parses_characters_and_episodes() {
        let parsed = parse_structured_content(FIXTURE);
        assert!(parsed.characters.len() >= 4);
        assert!(parsed.episodes.len() >= 3);
    }
}
