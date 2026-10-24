use super::*;

#[test]
fn ids_and_keys_are_unique() {
    for (i, a) in SYSTEMS.iter().enumerate() {
        for b in &SYSTEMS[i + 1..] {
            assert_ne!(a.id, b.id);
            assert!(!a.key.eq_ignore_ascii_case(b.key), "{}", a.key);
        }
    }
}

#[test]
fn dat_names_resolve() {
    let nes = from_dat_name("Nintendo - Nintendo Entertainment System (Headered) (Parent-Clone)");
    assert_eq!(nes.map(|s| s.key), Some("NES"));
    assert_eq!(
        from_dat_name("Nintendo - Game Boy").map(|s| s.key),
        Some("Game Boy")
    );
    assert_eq!(
        from_dat_name("Nintendo - Game Boy Color (20240101)").map(|s| s.key),
        Some("Game Boy Color")
    );
    assert_eq!(
        from_dat_name("Atari - Atari Lynx (LYX)").map(|s| s.key),
        Some("Lynx")
    );
    assert_eq!(from_dat_name("Something Else"), None);
}

#[test]
fn extensions_are_lowercase_and_not_archives() {
    for s in SYSTEMS {
        for e in s.extensions {
            assert_eq!(*e, e.to_ascii_lowercase(), "{}", s.key);
            assert!(!e.starts_with('.') && *e != "zip" && *e != "7z", "{}", s.key);
        }
    }
    assert!(is_rom_extension("SFC"));
    assert!(!is_rom_extension("mkv"));
    assert!(!is_rom_extension("url"));
    assert!(!is_rom_extension("nfo"));
}

#[test]
fn by_key_is_case_insensitive() {
    assert_eq!(by_key("snes").map(|s| s.id), Some(2));
    assert!(by_key("nope").is_none());
}

#[test]
fn icon_and_logo_names_are_plain_file_names() {
    for s in SYSTEMS {
        assert!(!s.icon.contains('/') && !s.icon.ends_with(".png"), "{}", s.key);
        let l = s.logo.to_ascii_lowercase();
        assert!(
            s.logo.is_empty()
                || (!l.contains('/') && [".svg", ".png", ".jpg", ".gif"].iter().any(|e| l.ends_with(e))),
            "{}: {}",
            s.key,
            s.logo
        );
        for q in s.wikidata {
            assert!(
                q.starts_with('Q') && q[1..].bytes().all(|b| b.is_ascii_digit()),
                "{}: {q}",
                s.key
            );
        }
    }
}

#[test]
fn own_icons_are_for_real_platforms() {
    for key in crate::enrich::own_icon_keys() {
        assert!(by_key(key).is_some(), "{key}");
    }
}

#[test]
fn sub_systems_are_among_the_retrobios_ids() {
    for s in SYSTEMS {
        for (id, label) in s.sub_systems {
            assert!(s.retrobios.contains(id), "{}: {id}", s.key);
            assert!(!label.is_empty());
        }
    }
}
