use super::*;

#[test]
fn a_manual_is_kept_by_game_and_page() {
    let t = tempfile::tempdir().unwrap();
    let media = t.path().join("Media");
    let conn = crate::db::open_in_memory().unwrap();
    let none = manual(&conn, "SNES", "Super Mario World (USA)").unwrap();
    assert_eq!(
        (none.path.as_ref(), none.checked, none.last_page),
        (None, false, 1)
    );

    let not_pdf = t.path().join("notes.txt");
    std::fs::write(&not_pdf, "hello").unwrap();
    assert!(
        add_manual(
            &conn,
            &media,
            "SNES",
            "Super Mario World (USA)",
            &not_pdf,
            "added by you"
        )
        .is_err()
    );

    let pdf = t.path().join("smw.pdf");
    std::fs::write(&pdf, b"%PDF-1.4 a manual").unwrap();
    let kept = add_manual(
        &conn,
        &media,
        "SNES",
        "Super Mario World (USA)",
        &pdf,
        "added by you",
    )
    .unwrap();
    assert_eq!(kept, media.join("SNES/manual/Super Mario World (USA).pdf"));
    assert!(pdf.exists(), "a copy: the user's file stays");
    set_page(&conn, "SNES", "Super Mario World (USA)", 7).unwrap();
    let info = manual(&conn, "SNES", "Super Mario World (USA)").unwrap();
    assert_eq!(
        (info.path, info.source.as_deref(), info.last_page),
        (Some(kept.clone()), Some("added by you"), 7)
    );

    remove_manual(&conn, &media, "SNES", "Super Mario World (USA)").unwrap();
    assert!(!kept.exists());
    assert_eq!(
        manual(&conn, "SNES", "Super Mario World (USA)").unwrap().path,
        None
    );
    // A title with characters a file name can't have.
    assert_eq!(
        manual_path(&media, "SNES", "Who: Me?"),
        media.join("SNES/manual/Who- Me-.pdf")
    );
}

#[test]
fn hasheous_s_answer_gives_the_vimm_id() {
    let answer = br#"{"name":"Super Mario World","attributes":[
        {"attributeName":"AIDescription","value":"..."},
        {"attributeName":"VIMMManualId","value":"4770","link":"https://vimm.net/manual/4770"}]}"#;
    assert_eq!(vimm_id_in(answer).as_deref(), Some("4770"));
    assert_eq!(vimm_id_in(br#"{"attributes":[]}"#), None);
    assert_eq!(vimm_id_in(b"not json"), None);
}
