use super::*;

#[test]
fn known_emulators_cover_the_systems_table() {
    for s in systems::SYSTEMS {
        for e in s.emulators {
            assert!(
                KNOWN_EMULATORS.iter().any(|k| k.name == *e),
                "{} lists unknown {e}",
                s.key
            );
        }
    }
    for e in KNOWN_EMULATORS {
        assert!(
            e.args.iter().any(|a| a.contains("{rom}") || a.contains("{set}")),
            "{}",
            e.name
        );
    }
}

#[test]
fn flatpak_commands() {
    let (p, a) = command(&Runner::Flatpak("org.x.Y".into()), vec!["{rom}".into()], None);
    assert_eq!(p, PathBuf::from("flatpak"));
    assert_eq!(a, ["run", "org.x.Y", "{rom}"]);
    let (_, a) = command(
        &Runner::Flatpak("org.x.Y".into()),
        vec!["/m/g/a.zip".into()],
        Some(Path::new("/m/g")),
    );
    assert_eq!(a, ["run", "--filesystem=/m/g", "org.x.Y", "/m/g/a.zip"]);
    let (p, a) = command(
        &Runner::Program("/bin/x".into()),
        vec!["r".into()],
        Some(Path::new("/m")),
    );
    assert_eq!(
        (p, a),
        (PathBuf::from("/bin/x"), vec!["r".to_owned()]),
        "a plain program needs none"
    );
    assert_eq!(
        fill(&["-e", "{rom}"], Path::new("/r/a b.iso"), "a b"),
        ["-e", "/r/a b.iso"]
    );
}
