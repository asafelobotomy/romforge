use super::*;
use crate::launch::{Ended, Playing, record_play};

fn ended(game: &str, started: u64, seconds: u64) -> Ended {
    Ended {
        playing: Playing {
            pid: 1,
            game: game.into(),
            system: "SNES".into(),
            emulator: "retroarch:snes9x".into(),
            started,
        },
        seconds,
        code: Some(0),
    }
}

#[test]
fn favourites_and_play_time_are_kept_per_game() {
    let lib = crate::Library::open_in_memory().unwrap();
    let conn = &lib.conn;
    set_favourite(conn, "SNES", "Mario", true).unwrap();
    set_favourite(conn, "SNES", "Zelda", true).unwrap();
    set_favourite(conn, "SNES", "Zelda", false).unwrap();
    for (game, at, secs) in [
        ("Mario", 100, 60),
        ("Zelda", 200, 30),
        ("Mario", 300, 120),
        ("Quick", 400, 3),
    ] {
        record_play(conn, &ended(game, at, secs)).unwrap();
    }
    let m = marks(conn).unwrap();
    assert_eq!(m.favourites, [("SNES".to_owned(), "Mario".to_owned())]);
    let played: Vec<_> = m
        .played
        .iter()
        .map(|p| (p.title.as_str(), p.seconds, p.sessions, p.last_played))
        .collect();
    assert_eq!(
        played,
        [("Mario", 180, 2, 300), ("Zelda", 30, 1, 200)],
        "latest first; a 3 s run isn't one"
    );
    assert_eq!(emulator(conn, "SNES", "Mario").unwrap(), None);
}
