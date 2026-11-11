use super::*;
use crate::view::{RowStatus, bare_row};

fn row(id: &str, title: &str, playable: bool) -> GameRow {
    let mut r = bare_row(id.into(), Some(6), title, RowStatus::Verified);
    r.playable = playable;
    r.name = crate::names::parse(title);
    r
}

fn view(rows: Vec<GameRow>) -> LibraryView {
    LibraryView {
        platforms: Vec::new(),
        rows,
    }
}

fn ids(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|h| h.id.as_str()).collect()
}

#[test]
fn queries_are_checked_and_folded() {
    assert_eq!(query("  Zelda: LINK  ").as_deref(), Some("zelda link"));
    assert_eq!(query("Pokémon").as_deref(), Some("pokémon"));
    assert_eq!(query("a\u{1}b"), None, "control characters");
    assert_eq!(query("a\nb"), None);
    assert_eq!(query(&"x".repeat(MAX_QUERY_BYTES + 1)), None);
    assert!(query(&"x".repeat(MAX_QUERY_BYTES)).is_some());
    assert_eq!(query(" - !"), None, "nothing to match");
}

#[test]
fn ids_are_checked() {
    assert!(id_shape("g:6:Super Mario World (USA)"));
    assert!(id_shape("f:12"));
    assert!(!id_shape(""));
    assert!(!id_shape("x:1"));
    assert!(!id_shape("/usr/bin/sh"));
    assert!(!id_shape("g:6:a\nb"));
    assert!(!id_shape(&format!("g:6:{}", "a".repeat(MAX_ID_BYTES))));
}

#[test]
fn best_matches_first() {
    let v = view(vec![
        row("g:6:a", "Super Mario World (USA)", true),
        row("g:6:b", "Mario Paint (Japan)", true),
        row("g:6:c", "Dr. Mario (World)", true),
        row("g:6:d", "Mario (Prototype)", true),
        row("g:6:e", "Super Marioland Deluxe (USA)", true),
        row("g:6:f", "World of Mario Kart Super (USA)", true),
        row("g:6:g", "F-Zero (USA)", true),
    ]);
    let hits = search(&v, "mario");
    assert_eq!(
        ids(&hits),
        ["g:6:d", "g:6:b", "g:6:c", "g:6:a", "g:6:e", "g:6:f"],
        "{hits:#?}"
    );
    assert_eq!(hits[0].score, 1.0);
    assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
    // Every word, in any order.
    assert_eq!(ids(&search(&v, "world mario")), ["g:6:a", "g:6:f"]);
    assert_eq!(ids(&search(&v, "fzero")), Vec::<&str>::new());
    assert_eq!(ids(&search(&v, "f zero")), ["g:6:g"]);
    assert!(search(&v, "").is_empty());
    assert!(search(&v, "zelda").is_empty());
}

#[test]
fn titles_subtitles_and_pictures() {
    let mut zelda = row(
        "g:6:z",
        "Legend of Zelda, The - A Link to the Past (USA) (Rev 1)",
        true,
    );
    zelda.pictures.insert("boxart".into(), "/m/zelda.png".into());
    let mut sf2 = row("g:6:sf2", "sf2", true);
    sf2.label = Some("Street Fighter II: The World Warrior (World 910522)".into());
    let v = view(vec![zelda, sf2]);
    let z = &search(&v, "link to the past")[0];
    assert_eq!(z.title, "The Legend of Zelda - A Link to the Past");
    assert!(z.subtitle.ends_with(" · USA · Rev 1"), "{}", z.subtitle);
    assert_eq!(z.picture.as_deref(), Some("/m/zelda.png"));
    let s = &search(&v, "street fighter")[0];
    assert_eq!(s.title, "Street Fighter II: The World Warrior");
    assert_eq!(s.picture, None);
}

#[test]
fn only_games_play_can_start() {
    let mut packed = row("g:6:p", "Monkey Island", true);
    packed.packed = true;
    let mut unmatched = row("f:9", "Monkey Island Demo.bin", true);
    unmatched.unmatched = true;
    let v = view(vec![
        row("g:6:a", "Monkey Island (USA)", true),
        row("f:3", "Monkey Island (Europe)", false),
        packed,
        unmatched,
    ]);
    assert_eq!(ids(&search(&v, "monkey")), ["g:6:a"]);
    assert!(find(&v, "g:6:a").is_some());
    for id in ["f:3", "g:6:p", "f:9", "g:6:missing", "", "g:6:a\n"] {
        assert!(find(&v, id).is_none(), "{id:?}");
    }
}

#[test]
fn versions_rank_under_their_main_row_and_hits_are_capped() {
    let mut eu = row("g:6:eu", "Tetris (Europe)", true);
    eu.main = false;
    let mut rows = vec![eu, row("g:6:us", "Tetris (USA)", true)];
    for i in 0..30 {
        rows.push(row(&format!("g:6:t{i:02}"), &format!("Tetris {i:02}"), true));
    }
    let hits = search(&view(rows), "tetris");
    assert_eq!(hits.len(), MAX_HITS);
    assert_eq!(ids(&hits[..2]), ["g:6:us", "g:6:eu"]);
}
