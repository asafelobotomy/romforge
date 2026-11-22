use super::*;
use crate::view::{Category, RowStatus, bare_row};

fn v(id: &str, title: &str, main: bool, category: Option<Category>, playable: bool) -> GameRow {
    let mut r = bare_row(id.into(), None, title, RowStatus::Verified);
    r.system = Some("SNES".into());
    r.group = Some("v:1:t:Mario".into());
    r.main = main;
    r.category = category;
    r.playable = playable;
    r
}

fn rows() -> Vec<GameRow> {
    let mut lone = bare_row("f:9".into(), None, "Zelda (USA)", RowStatus::Verified);
    lone.system = Some("SNES".into());
    lone.playable = true;
    vec![
        v("f:3", "Mario (Japan)", false, Some(Category::Region), true),
        v("f:4", "Mario (USA) (Hack)", false, Some(Category::Hack), true),
        v("f:1", "Mario (USA)", true, None, true),
        v(
            "f:2",
            "Mario (Europe) (Rev 1)",
            false,
            Some(Category::Region),
            true,
        ),
        v(
            "x:1:Mario (Beta)",
            "Mario (Beta)",
            false,
            Some(Category::Edition),
            false,
        ),
        lone,
    ]
}

#[test]
fn play_asks_which_version_until_one_is_always_run() {
    let lib = crate::Library::open_in_memory().unwrap();
    let (conn, rows) = (&lib.conn, rows());
    let c = play_choice(conn, &rows, "f:1").unwrap();
    assert_eq!(c.run, None);
    let ids: Vec<_> = c.versions.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        ["f:1", "f:2", "f:3", "f:4"],
        "main first, then as the list shows them; not the missing beta"
    );

    // A version picked on its own, and a game without versions, start as they are.
    assert_eq!(
        play_choice(conn, &rows, "f:3").unwrap().run.as_deref(),
        Some("f:3")
    );
    assert_eq!(
        play_choice(conn, &rows, "f:9").unwrap().run.as_deref(),
        Some("f:9")
    );

    set_always_run(conn, &rows, "f:1", Some("f:3")).unwrap();
    assert_eq!(
        play_choice(conn, &rows, "f:1").unwrap().run.as_deref(),
        Some("f:3")
    );
    assert_eq!(
        always_run(conn, &rows, &rows[1]).unwrap().map(|r| r.id.as_str()),
        Some("f:3")
    );
    assert_eq!(
        always_run_list(conn).unwrap(),
        [AlwaysRun {
            system: "SNES".into(),
            group: "v:1:t:Mario".into(),
            title: "Mario (Japan)".into()
        }]
    );

    // Kept by title: a rescan that renumbers the rows still finds it.
    let mut again = rows.clone();
    again[0].id = "f:30".into();
    assert_eq!(
        play_choice(conn, &again, "f:1").unwrap().run.as_deref(),
        Some("f:30")
    );
    // Gone from the library: Play asks again.
    let gone: Vec<_> = rows.iter().filter(|r| r.id != "f:3").cloned().collect();
    assert_eq!(play_choice(conn, &gone, "f:1").unwrap().run, None);

    set_always_run(conn, &rows, "f:1", None).unwrap();
    assert_eq!(play_choice(conn, &rows, "f:1").unwrap().run, None);
    assert!(always_run_list(conn).unwrap().is_empty());
}

#[test]
fn always_run_takes_only_a_playable_version_of_the_same_game() {
    let lib = crate::Library::open_in_memory().unwrap();
    let (conn, rows) = (&lib.conn, rows());
    assert!(
        set_always_run(conn, &rows, "f:1", Some("f:9")).is_err(),
        "another game"
    );
    assert!(
        set_always_run(conn, &rows, "f:1", Some("x:1:Mario (Beta)")).is_err(),
        "not playable"
    );
    assert!(set_always_run(conn, &rows, "f:9", None).is_err(), "no versions");
    assert!(always_run_list(conn).unwrap().is_empty());
}
