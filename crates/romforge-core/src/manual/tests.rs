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

#[test]
fn marks_are_kept_by_page_and_checked() {
    let conn = crate::db::open_in_memory().unwrap();
    let hl = NewMark {
        page: 3,
        kind: MarkKind::Highlight,
        text: "  Press START to pause.  ".into(),
        rects: vec![[0.1, 0.2, 0.5, 0.03], [0.1, 0.23, 0.3, 0.03]],
        color: "yellow".into(),
        note: String::new(),
    };
    let area = NewMark {
        page: 1,
        kind: MarkKind::Area,
        text: String::new(),
        rects: vec![[0.4, 0.4, 0.2, 0.2]],
        color: "red".into(),
        note: "the secret exit".into(),
    };
    let h = add_mark(&conn, "SNES", "Super Mario World (USA)", &hl).unwrap();
    add_mark(&conn, "SNES", "Super Mario World (USA)", &area).unwrap();
    let all = marks(&conn, "SNES", "Super Mario World (USA)").unwrap();
    assert_eq!(all.iter().map(|m| m.page).collect::<Vec<_>>(), [1, 3], "by page");
    assert_eq!(all[1].text, "Press START to pause.");
    assert_eq!(all[1].rects.len(), 2);
    assert_eq!(all[0].note, "the secret exit");

    edit_mark(&conn, h, "pause menu", "green").unwrap();
    let edited = marks(&conn, "SNES", "Super Mario World (USA)").unwrap();
    assert_eq!(
        (edited[1].note.as_str(), edited[1].color.as_str()),
        ("pause menu", "green")
    );
    remove_mark(&conn, h).unwrap();
    assert_eq!(marks(&conn, "SNES", "Super Mario World (USA)").unwrap().len(), 1);

    let bad = |f: &dyn Fn(&mut NewMark)| {
        let mut m = hl.clone();
        f(&mut m);
        add_mark(&conn, "SNES", "X", &m).is_err()
    };
    assert!(bad(&|m| m.color = "purple".into()));
    assert!(bad(&|m| m.rects = vec![[0.5, 0.5, 2.0, 0.1]]));
    assert!(bad(&|m| m.rects.clear()));
    assert!(bad(&|m| m.text = "   ".into()), "a highlight needs text");
    assert!(bad(&|m| m.page = 0));
}

#[test]
fn ocr_words_are_kept_per_page_and_language() {
    let t = tempfile::tempdir().unwrap();
    let media = t.path().join("Media");
    let conn = crate::db::open_in_memory().unwrap();
    let w = |text: &str, x: f64| OcrWord {
        text: text.into(),
        rect: [x, 0.1, 0.1, 0.02],
        conf: 91.0,
        eol: false,
    };
    save_ocr(&conn, "SNES", "G", 2, "eng", &[w("Press", 0.1), w("START", 0.2)]).unwrap();
    save_ocr(&conn, "SNES", "G", 1, "eng", &[]).unwrap();
    assert_eq!(ocr_page(&conn, "SNES", "G", 2, "eng").unwrap().unwrap().len(), 2);
    assert_eq!(
        ocr_page(&conn, "SNES", "G", 2, "jpn").unwrap(),
        None,
        "per language"
    );
    assert_eq!(
        ocr_page(&conn, "SNES", "G", 1, "eng").unwrap(),
        Some(vec![]),
        "a page with no words is known"
    );
    assert_eq!(
        ocr_texts(&conn, "SNES", "G", "eng").unwrap(),
        [(1, String::new()), (2, "Press START".into())]
    );
    let mut out = w("x", 0.5);
    out.rect[2] = 3.0;
    assert!(save_ocr(&conn, "SNES", "G", 3, "eng", &[out]).is_err());

    // A new manual forgets the old one's OCR.
    let pdf = t.path().join("g.pdf");
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();
    add_manual(&conn, &media, "SNES", "G", &pdf, "added by you").unwrap();
    assert_eq!(ocr_page(&conn, "SNES", "G", 2, "eng").unwrap(), None);

    let langs = ocr_langs(t.path());
    assert!(langs.iter().any(|l| l.code == "eng" && !l.downloaded));
    assert!(download_ocr_lang(t.path(), "klingon").is_err());
}
