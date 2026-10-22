use super::*;

fn theme(repo: &Path, id: &str, version: &str, css: &str) {
    let d = repo.join("themes").join(id);
    std::fs::create_dir_all(d.join("img")).unwrap();
    std::fs::write(
        d.join("theme.json"),
        format!(
            r##"{{"api": 1, "id": "{id}", "name": "Mine", "version": "{version}", "licence": "CC0-1.0",
                "schemes": {{"a": {{"--fs-bg": "#000"}}}}}}"##
        ),
    )
    .unwrap();
    std::fs::write(d.join("theme.css"), css).unwrap();
    std::fs::write(d.join("img/bg.png"), b"png").unwrap();
}

fn publish(repo: &Path) -> Index {
    let index = build_index(repo).unwrap();
    std::fs::write(repo.join("themes.json"), index_text(&index)).unwrap();
    index
}

#[test]
fn an_index_is_built_installed_updated_and_removed() {
    let t = tempfile::tempdir().unwrap();
    let (repo, dir) = (t.path().join("repo"), t.path().join("Themes"));
    theme(&repo, "mine", "1.0.0", "a { color: red }");
    let index = publish(&repo);
    assert_eq!(index.themes.len(), 1);
    let r = &index.themes[0];
    assert_eq!(
        r.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["img/bg.png", "theme.css", "theme.json"]
    );

    let source = Source::parse(repo.to_str().unwrap());
    let got = source.index().unwrap();
    assert_eq!(got, index);
    assert_eq!(status(Some(&dir), r), Status::NotInstalled);

    let mut seen = 0;
    install(&dir, &source, r, |done, total| {
        assert!(done <= total);
        seen = done;
    })
    .unwrap();
    assert_eq!(seen, r.size());
    assert_eq!(
        status(Some(&dir), r),
        Status::Installed {
            version: "1.0.0".into()
        }
    );
    assert!(
        crate::themes::find(Some(&dir), "mine").is_some(),
        "usable once in"
    );
    assert!(!dir.join(".download-mine").exists() && !dir.join(".check-mine").exists());

    // A new version: offered, and swapped in.
    theme(&repo, "mine", "1.1.0", "a { color: blue }");
    let r2 = publish(&repo).themes.remove(0);
    assert_eq!(
        status(Some(&dir), &r2),
        Status::UpdateAvailable {
            installed: "1.0.0".into()
        }
    );
    install(&dir, &source, &r2, |_, _| {}).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("mine/theme.css")).unwrap(),
        "a { color: blue }"
    );

    remove(&dir, "mine").unwrap();
    assert!(!dir.join("mine").exists());
}

#[test]
fn a_file_that_isnt_the_listed_one_leaves_the_theme_there() {
    let t = tempfile::tempdir().unwrap();
    let (repo, dir) = (t.path().join("repo"), t.path().join("Themes"));
    theme(&repo, "mine", "1.0.0", "a { color: red }");
    let source = Source::parse(repo.to_str().unwrap());
    install(&dir, &source, &publish(&repo).themes[0], |_, _| {}).unwrap();

    // The index says 1.1.0, but the file served differs from its hash.
    theme(&repo, "mine", "1.1.0", "a { color: blue }");
    let r = publish(&repo).themes.remove(0);
    // Same size, other bytes: the hash catches it.
    std::fs::write(repo.join("themes/mine/theme.css"), "a { color: navy }").unwrap();
    let e = install(&dir, &source, &r, |_, _| {}).unwrap_err().to_string();
    assert!(e.contains("isn't the file the index lists"), "{e}");
    assert_eq!(
        std::fs::read_to_string(dir.join("mine/theme.css")).unwrap(),
        "a { color: red }"
    );
    assert!(!dir.join(".download-mine").exists());

    // A theme romforge's checks refuse is refused at publishing, and at
    // install if an index lists it anyway.
    theme(&repo, "bad", "1.0.0", "@import 'x.css';");
    assert!(
        build_index(&repo)
            .unwrap_err()
            .to_string()
            .contains("bad: theme.css: no @import")
    );
}

#[test]
fn a_theme_of_your_own_is_never_overwritten_or_removed() {
    let t = tempfile::tempdir().unwrap();
    let (repo, dir) = (t.path().join("repo"), t.path().join("Themes"));
    theme(&repo, "mine", "1.0.0", "");
    let r = publish(&repo).themes.remove(0);
    // Hand-made, same name.
    theme(t.path(), "mine", "0.1.0", "b { color: red }");
    std::fs::rename(t.path().join("themes"), &dir).unwrap();
    assert_eq!(status(Some(&dir), &r), Status::Taken);
    let source = Source::parse(repo.to_str().unwrap());
    assert!(install(&dir, &source, &r, |_, _| {}).is_err());
    assert!(remove(&dir, "mine").is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("mine/theme.css")).unwrap(),
        "b { color: red }"
    );
    // A built-in theme's name is taken too.
    let builtin = Remote {
        id: "snes-uk".into(),
        ..r.clone()
    };
    assert_eq!(status(Some(&dir), &builtin), Status::Taken);
}

#[test]
fn index_entries_are_checked() {
    let ok = Remote {
        id: "mine".into(),
        name: "Mine".into(),
        version: "1".into(),
        api: 1,
        author: String::new(),
        licence: "MIT".into(),
        licence_url: String::new(),
        description: String::new(),
        preview: String::new(),
        files: ["theme.json", "theme.css"]
            .iter()
            .map(|p| RemoteFile {
                path: (*p).into(),
                size: 1,
                sha256: "0".repeat(64),
            })
            .collect(),
    };
    assert_eq!(ok.problem(), None);
    let with = |f: &dyn Fn(&mut Remote)| {
        let mut r = ok.clone();
        f(&mut r);
        r.problem().unwrap()
    };
    assert!(with(&|r| r.api = 2).contains("API"));
    assert!(with(&|r| r.licence.clear()).contains("licence"));
    let extra = |path: &str| {
        let path = path.to_owned();
        with(&move |r: &mut Remote| {
            r.files.push(RemoteFile {
                path: path.clone(),
                ..r.files[0].clone()
            })
        })
    };
    assert!(extra("../x.png").contains("not a file"));
    assert!(extra("run.sh").contains("not a file"));
    assert!(extra(".hidden.png").contains("not a file"));
    assert!(with(&|r| r.files[1].size = crate::themes::MAX_TOTAL + 1).contains("MB"));
    assert!(with(&|r| r.files.truncate(1)).contains("theme.css"));
    assert!(Source::Folder("/x".into()).get("../etc/passwd", 10).is_err());

    assert!(
        newer("1.10.0", "1.9.2") && newer("2", "1.9") && !newer("1.0", "1.0.0") && !newer("1.0.0", "1.1")
    );
}
