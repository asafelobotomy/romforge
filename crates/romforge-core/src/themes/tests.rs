use super::*;

fn manifest(id: &str, extra: &str) -> String {
    format!(
        r##"{{"api": 1, "id": "{id}", "name": "Mine", "schemes": {{"a": {{"--fs-bg": "#000"}}}}{extra}}}"##
    )
}

fn theme(dir: &Path, id: &str, json: &str, css: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(dir.join(id).join("theme.json"), json).unwrap();
    std::fs::write(dir.join(id).join("theme.css"), css).unwrap();
}

#[test]
fn the_built_in_themes_are_all_usable() {
    let all = list(None);
    assert_eq!(all.len(), BUILTIN.len());
    for e in &all {
        assert!(e.usable(), "{}: {:?}", e.id, e.problem);
        assert!(e.builtin);
    }
    assert!(find(None, DEFAULT).is_some());
}

#[test]
fn manifests_are_refused_with_a_reason() {
    let refused = |json: &str| parse(json, "mine").unwrap_err();
    assert!(refused(&manifest("mine", "").replace("\"api\": 1", "\"api\": 2")).contains("API"));
    assert!(refused(&manifest("other", "")).contains("folder"));
    assert!(refused(&manifest("mine", ", \"layout\": \"wheel\"")).contains("layout"));
    assert!(refused(&manifest("mine", "").replace("--fs-bg", "--bg")).contains("--fs-"));
    assert!(refused(&manifest("mine", "").replace("#000", "url(x.png)")).contains("plain value"));
    assert!(refused(&manifest("mine", "").replace("--fs-bg", "--fs-scale")).contains("text size"));
    assert!(refused(&manifest("mine", ", \"script\": \"x.js\"")).contains("unknown field"));
    assert!(parse(&manifest("mine", ""), "mine").is_ok());
    let flow = ", \"layout\": \"cover-flow\", \"layouts\": [\"grid\", \"list\"]";
    assert!(parse(&manifest("mine", flow), "mine").is_ok());
}

#[test]
fn css_that_reaches_out_is_refused() {
    for (css, why) in [
        ("@import 'x.css';", "@import"),
        ("a { background: url(https://x/y.png) }", "//"),
        ("a { background: url(/etc/passwd) }", "in the theme"),
        ("a { background: url(../other/x.png) }", "in the theme"),
        ("a { background: url(data:image/png;base64,AA) }", "in the theme"),
        ("@\\69mport 'x';", "escapes"),
        ("} body { display: none }", "no {"),
        ("a { color: red", "isn't closed"),
        ("a { b: expression(alert(1)) }", "expression"),
        // A brace inside an unquoted url() is part of the url to the
        // browser, so it would close the @scope: refused.
        ("a{background:url({)} } body{display:none}", "in the theme"),
        ("a{background:url( img/x.png;})}", "in the theme"),
    ] {
        let e = check_css(css).unwrap_err();
        assert!(e.contains(why), "{css}: {e}");
    }
    // Braces and slashes in comments and strings don't count.
    assert!(check_css("/* } // */ a { content: \"}\"; background: url('img/bg.png') }").is_ok());
}

#[test]
fn user_themes_are_served_inside_their_folder_and_scope() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    theme(
        dir,
        "mine",
        &manifest("mine", ""),
        "a { background: url(img/bg.png) }",
    );
    std::fs::create_dir_all(dir.join("mine/img")).unwrap();
    std::fs::write(dir.join("mine/img/bg.png"), b"png").unwrap();
    std::fs::write(dir.join("secret.png"), b"no").unwrap();
    theme(dir, "snes-uk", &manifest("snes-uk", ""), "");
    theme(dir, "bad", &manifest("bad", ""), "@import 'x';");

    let all = list(Some(dir));
    let get = |id: &str| all.iter().find(|e| e.id == id && !e.builtin).unwrap();
    assert!(get("mine").usable());
    assert!(get("snes-uk").problem.as_deref().unwrap().contains("built-in"));
    assert!(get("bad").problem.as_deref().unwrap().contains("@import"));

    let (css, mime) = file(Some(dir), "mine", "theme.css").unwrap();
    assert_eq!(mime, "text/css");
    let css = String::from_utf8(css).unwrap();
    assert!(css.starts_with("@scope ([data-fs-root]) {"), "{css}");
    assert_eq!(file(Some(dir), "mine", "img/bg.png").unwrap().0, b"png");
    for path in [
        "../secret.png",
        "/etc/passwd",
        "img/../../secret.png",
        "theme.json",
        "x.js",
    ] {
        assert!(file(Some(dir), "mine", path).is_none(), "{path}");
    }
    assert!(file(Some(dir), "bad", "theme.css").is_none());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("secret.png"), dir.join("mine/img/link.png")).unwrap();
        assert!(file(Some(dir), "mine", "img/link.png").is_none(), "no links out");
    }
    // Built-in CSS is served from the binary, scoped too.
    let (css, _) = file(None, "game-boy", "theme.css").unwrap();
    assert!(String::from_utf8(css).unwrap().contains("@scope"));

    assert_eq!(stamp(Some(dir), "snes-uk"), 0, "a built-in name never reloads");
    assert!(stamp(Some(dir), "mine") > 0);
}

#[test]
fn the_pick_fits_the_themes_there_are() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
        .unwrap();
    assert_eq!(load(&conn).unwrap(), Selection::default());

    let mut s = Selection {
        theme: "game-boy".into(),
        scheme: "night".into(),
        layout: "list".into(),
        text_size: 115,
        ..Selection::default()
    };
    s.options
        .insert("game-boy".into(), [("art".to_owned(), "plain".to_owned())].into());
    save(&conn, &s).unwrap();
    assert_eq!(load(&conn).unwrap(), s);
    let a = applied(None, &s);
    assert_eq!((a.theme.as_str(), a.layout.as_str()), ("game-boy", "list"));
    assert_eq!(a.scheme.as_deref(), Some("night"));
    assert_eq!(a.attrs.get("art").map(String::as_str), Some("plain"));
    assert!((a.scale - 1.15).abs() < 1e-6);

    // Gone: the default, which follows the system's light and dark.
    let gone = Selection {
        theme: "nowhere".into(),
        text_size: 77,
        ..Selection::default()
    };
    let a = applied(None, &gone);
    assert_eq!(a.theme, DEFAULT);
    assert_eq!(a.scheme, None);
    assert_eq!(a.schemes.keys().collect::<Vec<_>>(), ["dark", "light"]);
    assert_eq!(
        a.attrs.get("corners").map(String::as_str),
        Some("round"),
        "first choice"
    );
    assert_eq!(a.scale, 1.0);
    // An unknown option value or scheme falls back to the theme's own.
    let mut odd = s.clone();
    odd.scheme = "pink".into();
    odd.options
        .insert("game-boy".into(), [("art".to_owned(), "neon".to_owned())].into());
    let a = applied(None, &odd);
    assert_eq!(a.scheme.as_deref(), Some("dmg"));
    assert_eq!(a.attrs.get("art").map(String::as_str), Some("tinted"));
}

#[test]
fn the_downloader_s_dot_folders_are_not_themes_and_served_css_is_checked_css() {
    let t = tempfile::tempdir().unwrap();
    theme(t.path(), ".download-mine", &manifest("mine", ""), "");
    theme(t.path(), "mine", &manifest("mine", ""), "a { color: red }");
    let all = list(Some(t.path()));
    assert!(!all.iter().any(|e| e.id.starts_with('.')), "a crash's leftovers");
    let (css, _) = file(Some(t.path()), "mine", "theme.css").unwrap();
    assert_eq!(String::from_utf8(css).unwrap(), scoped("a { color: red }"));
    std::fs::write(t.path().join("mine/theme.css"), "@import 'x.css';").unwrap();
    assert!(file(Some(t.path()), "mine", "theme.css").is_none());
    let (css, _) = file(Some(t.path()), DEFAULT, "theme.css").unwrap();
    assert!(!css.is_empty(), "a built-in one");
}
