#[test]
fn secrets_are_masked_and_kept() {
    let stored = Keys {
        steamgriddb_key: Some("real-key".into()),
        igdb_client_id: Some("my-app".into()),
        igdb_client_secret: Some("real-secret".into()),
        ..Keys::default()
    };
    let shown = stored.masked();
    assert_eq!(shown.steamgriddb_key.as_deref(), Some(MASK));
    assert_eq!(
        shown.igdb_client_id.as_deref(),
        Some("my-app"),
        "ids are not secret"
    );
    assert_eq!(shown.screenscraper_password, None, "unset stays unset");
    // The form sends the mask back unchanged, and a new secret.
    let sent = Keys {
        igdb_client_secret: Some("new-secret".into()),
        ..shown
    };
    let saved = sent.unmasked(&stored);
    assert_eq!(saved.steamgriddb_key.as_deref(), Some("real-key"));
    assert_eq!(saved.igdb_client_secret.as_deref(), Some("new-secret"));
}

use super::*;

fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let (s, l) = (t.path().join("source"), t.path().join("library"));
    std::fs::create_dir_all(&s).unwrap();
    std::fs::create_dir_all(&l).unwrap();
    (t, s, l)
}

#[test]
fn save_and_load_round_trip() {
    let (t, s, l) = dirs();
    let mut conn = crate::db::open_in_memory().unwrap();
    assert_eq!(load(&conn).unwrap(), Settings::default());
    let stored = save(
        &mut conn,
        &Settings {
            source_dir: Some(s.clone()),
            library_dir: Some(l.clone()),
            keep_originals: true,
            staging_dir: Some(t.path().join("stage")),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(load(&conn).unwrap(), stored);
    assert_eq!(stored.source_dir, Some(s.canonicalize().unwrap()));
    // Clearing the library (and so keep originals) removes them.
    let cleared = save(
        &mut conn,
        &Settings {
            source_dir: Some(s),
            ..Settings::default()
        },
    )
    .unwrap();
    assert_eq!(load(&conn).unwrap(), cleared);
    assert_eq!(cleared.library_dir, None);
    assert!(!cleared.keep_originals);
}

#[test]
fn keys_are_trimmed_paired_and_masked() {
    let mut conn = crate::db::open_in_memory().unwrap();
    let mut s = Settings::default();
    s.keys.steamgriddb_key = Some("  abcdef0123456789  ".into());
    s.keys.igdb_client_id = Some("id".into());
    assert!(save(&mut conn, &s).unwrap_err().to_string().contains("IGDB"));
    s.keys.igdb_client_secret = Some("secret".into());
    s.keys.screenscraper_user = Some("".into());
    let stored = save(&mut conn, &s).unwrap();
    assert_eq!(stored.keys.steamgriddb_key.as_deref(), Some("abcdef0123456789"));
    assert_eq!(stored.keys.screenscraper_user, None, "empty means unset");
    assert!(stored.keys.igdb() && stored.keys.steamgriddb() && !stored.keys.screenscraper());
    assert_eq!(load(&conn).unwrap(), stored);
    assert_eq!(mask("abcdef0123456789"), "abc…789");
    assert_eq!(mask("short"), "•••••");
}

#[test]
fn folders_must_make_sense_together() {
    let (t, s, l) = dirs();
    let err = |s: Settings| s.validated().unwrap_err().to_string();
    assert!(
        err(Settings {
            source_dir: Some(t.path().join("nope")),
            ..Default::default()
        })
        .contains("Source folder")
    );
    assert!(
        err(Settings {
            source_dir: Some(s.clone()),
            library_dir: Some(s.join(".")),
            ..Default::default()
        })
        .contains("inside one another")
    );
    let inner = s.join("sorted");
    std::fs::create_dir(&inner).unwrap();
    assert!(
        err(Settings {
            source_dir: Some(s.clone()),
            library_dir: Some(inner),
            ..Default::default()
        })
        .contains("inside")
    );
    assert!(
        err(Settings {
            source_dir: Some(s.clone()),
            staging_dir: Some(s.join("stage")),
            ..Default::default()
        })
        .contains("staging")
    );
    assert!(
        err(Settings {
            source_dir: Some(s.clone()),
            keep_originals: true,
            ..Default::default()
        })
        .contains("Library")
    );
    assert!(
        Settings {
            source_dir: Some(s),
            library_dir: Some(l),
            keep_originals: true,
            ..Default::default()
        }
        .validated()
        .is_ok()
    );
}

#[test]
fn while_playing_is_one_of_three() {
    let s = Settings {
        while_playing: "minimise".into(),
        ..Settings::default()
    };
    assert_eq!(s.validated().unwrap().while_playing, "minimise");
    assert_eq!(Settings::default().while_playing, "tray");
    let blank = Settings {
        while_playing: " ".into(),
        ..Settings::default()
    };
    assert_eq!(blank.validated().unwrap().while_playing, "tray");
    let odd = Settings {
        while_playing: "vanish".into(),
        ..Settings::default()
    };
    assert!(odd.validated().is_err());
}
