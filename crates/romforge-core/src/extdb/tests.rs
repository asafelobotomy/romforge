use super::*;

#[test]
fn keys_fold_accents_and_tags() {
    assert_eq!(
        crate::wikidata::normalize("Pokémon: Emerald Version"),
        crate::wikidata::normalize("Pokemon - Emerald Version (USA, Europe)")
    );
    assert_eq!(
        crate::wikidata::normalize("The Legend of Zelda: A Link to the Past"),
        crate::wikidata::normalize("Legend of Zelda, The - A Link to the Past (USA)")
    );
}

#[test]
fn dates_and_regions() {
    assert_eq!(parse_us_date("Aug 13, 1991"), Some((1991, Some(8), Some(13))));
    assert_eq!(parse_us_date("1994"), Some((1994, None, None)));
    assert_eq!(parse_us_date("soon"), None);
    assert_eq!(
        regions_of("Pokemon - Emerald Version (USA, Europe)"),
        ["North America", "United States", "Europe"]
    );
    assert!(regions_of("Tetris").is_empty());
}

#[test]
fn wikipedia_titles_are_decoded_once() {
    assert_eq!(
        wikipedia_title("https://en.wikipedia.org/wiki/Pok%C3%A9mon_Red_and_Blue#Plot").as_deref(),
        Some("Pokémon Red and Blue")
    );
    assert_eq!(
        wikipedia_title("https://en.wikipedia.org/wiki/Hey_You,_Pikachu!").as_deref(),
        Some("Hey You, Pikachu!")
    );
    assert_eq!(wikipedia_title("https://example.org/x"), None);
    assert_eq!(
        crate::fetch::decode_segment("100%_a%2"),
        "100%_a%2",
        "stray % kept"
    );
}
