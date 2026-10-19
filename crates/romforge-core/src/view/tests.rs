use super::patches::bare_row;
use super::shown_value;

#[test]
fn libretro_unknowns() {
    assert_eq!(
        shown_value("Nintendo ???").as_deref(),
        Some("Nintendo (unconfirmed)")
    );
    assert_eq!(shown_value("???"), None);
    assert_eq!(shown_value("  "), None);
    assert_eq!(
        shown_value("Ape, HAL Laboratory").as_deref(),
        Some("Ape, HAL Laboratory")
    );
}

use super::*;

#[test]
fn rows_sort_by_year_with_unknown_years_last() {
    let row = |t: &str, y: Option<i32>| {
        let mut r = bare_row(t.into(), Some(6), t, RowStatus::Verified);
        r.year = y;
        r
    };
    let mut rows = vec![row("B", Some(1991)), row("A", None), row("C", Some(1989))];
    let titles = |rows: &[GameRow]| rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>();
    sort_rows(&mut rows, SortBy::Year, false);
    assert_eq!(titles(&rows), ["C", "B", "A"]);
    sort_rows(&mut rows, SortBy::Year, true);
    assert_eq!(titles(&rows), ["B", "C", "A"], "unknown last both ways");
    sort_rows(&mut rows, SortBy::Title, true);
    assert_eq!(titles(&rows), ["C", "B", "A"]);
}

#[test]
fn search_titles() {
    assert_eq!(
        search_title("Legend of Zelda, The - A Link to the Past (USA)"),
        "The Legend of Zelda - A Link to the Past"
    );
    assert_eq!(search_title("Super Mario World (USA) [!]"), "Super Mario World");
    assert_eq!(
        search_title("Revenge of Shinobi, The (USA, Europe)"),
        "The Revenge of Shinobi"
    );
}

#[test]
fn short_names() {
    assert_eq!(short_name("SNES"), "SNES");
    assert_eq!(short_name("Game Boy Advance"), "GBA");
    assert_eq!(short_name("PlayStation 2"), "PS2");
    assert_eq!(short_name("PlayStation"), "PS");
    assert_eq!(short_name("Commodore 64"), "C64");
    assert_eq!(short_name("Genesis"), "GEN");
}

#[test]
fn links_are_search_urls_or_direct() {
    let mut f = BTreeMap::new();
    let l = links(
        "Super Mario World (USA)",
        Some("Super Nintendo Entertainment System"),
        &f,
    );
    assert_eq!(
        l[0].url,
        "https://www.mobygames.com/search/?q=Super%20Mario%20World"
    );
    assert!(l.iter().all(|x| !x.direct));
    f.insert("mobygames_id".to_owned(), "123".to_owned());
    f.insert("wikipedia".to_owned(), "Super Mario World".to_owned());
    f.insert("retroachievements_id".to_owned(), "228".to_owned());
    f.insert("wikidata".to_owned(), "Q1".to_owned());
    let l = links("Super Mario World (USA)", None, &f);
    assert_eq!(l[0].url, "https://www.mobygames.com/game/123/");
    assert_eq!(l[1].url, "https://retroachievements.org/game/228");
    assert_eq!(l[2].url, "https://en.wikipedia.org/wiki/Super_Mario_World");
    assert!(l[3].url.contains("igdb.com/search"), "no IGDB id: still a search");
    assert_eq!(l.last().unwrap().url, "https://www.wikidata.org/wiki/Q1");
}
