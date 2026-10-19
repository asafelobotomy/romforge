use super::find::{first_year, strip_tags};
use super::*;

#[test]
fn similarity_ranks_by_shared_words() {
    let q = "Dragon Quest III (English v2.0)[Addendum]";
    let a = similarity(q, "Dragon Quest III - Soshite Densetsu e... (Japan)");
    let b = similarity(q, "Dragon Quest I & II (Japan)");
    let c = similarity(q, "Super Mario World (USA)");
    assert_eq!(a, 0.9, "same game, subtitle missing");
    assert_eq!(b, 0.0, "numerals differ");
    assert_eq!(c, 0.0);
}

#[test]
fn wikidata_search_results() {
    let body = br#"{"search":[
        {"id":"Q853143","label":"Super Mario World","description":"1990 video game"},
        {"id":"Q1","label":"Super Mario World 2: Yoshi's Island","description":"video game from 1995"},
        {"id":"Q2","label":"Nameless"}]}"#;
    let r = parse_entity_search(body, "Super Mario World", Some("SNES")).unwrap();
    assert_eq!(r.len(), 3);
    assert_eq!(
        (r[0].id.as_str(), r[0].year, r[0].score),
        ("Q853143", Some(1990), 1.0)
    );
    assert_eq!(r[1].year, Some(1995));
    assert_eq!(r[2].year, None);
    assert_eq!(r[0].system.as_deref(), Some("SNES"));
}

#[test]
fn tags_and_articles_go_for_wikidata() {
    assert_eq!(strip_tags("Super Mario World (USA) (Rev 1)"), "Super Mario World");
    assert_eq!(
        strip_tags("Legend of Zelda, The - A Link to the Past (USA)"),
        "The Legend of Zelda: A Link to the Past"
    );
    assert_eq!(
        strip_tags("Castlevania - Dracula X (USA)"),
        "Castlevania: Dracula X"
    );
    assert_eq!(strip_tags("Tetris"), "Tetris");
    assert_eq!(first_year("ca. 12345 and 1991"), Some(1991));
}
