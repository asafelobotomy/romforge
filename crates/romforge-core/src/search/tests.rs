use super::find::{first_year, parse_entities, parse_page_ids, strip_tags};
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
fn platform_search_keeps_qid_order_and_mul_labels() {
    let pages = br#"{"query":{"search":[
        {"title":"Q1045765"},{"title":"Q853143"},{"title":"NotAQid"}]}}"#;
    assert_eq!(parse_page_ids(pages).unwrap(), ["Q1045765", "Q853143"]);
    let ents = br#"{"entities":{
        "Q853143":{"labels":{"mul":{"value":"Super Mario World"}},
                   "descriptions":{"en":{"value":"1990 platform video game"}}},
        "Q1045765":{"labels":{"en":{"value":"Super Mario World 2: Yoshi's Island"}},
                    "descriptions":{"en":{"value":"1995 video game"}}}
    }}"#;
    let r = parse_entities(
        ents,
        "Super Mario World",
        Some("SNES"),
        &["Q1045765".into(), "Q853143".into()],
    )
    .unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].id, "Q1045765");
    assert_eq!(r[1].title, "Super Mario World");
    assert_eq!(r[1].year, Some(1990));
    assert_eq!(r[1].system.as_deref(), Some("SNES"));
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
    assert_eq!(
        strip_tags("Boy and His Blob, A - Trouble on Blobolonia (USA)"),
        "A Boy and His Blob: Trouble on Blobolonia"
    );
    assert_eq!(
        strip_tags("Mario, Theodore and Friends (USA)"),
        "Mario, Theodore and Friends",
        "only a trailing article moves"
    );
    assert_eq!(first_year("ca. 12345 and 1991"), Some(1991));
}
