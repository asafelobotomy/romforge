use super::*;

#[test]
fn cache_names_never_collide_or_overflow() {
    assert_eq!(cache_stem("Super Mario World (USA)"), "Super Mario World (USA)");
    assert_ne!(cache_stem("A: B"), cache_stem("A? B"));
    assert!(cache_stem("A: B").starts_with("A_ B ["));
    let long = "Ä".repeat(150);
    let stem = cache_stem(&long);
    assert!(stem.len() <= 211 && stem.ends_with(']'), "{stem}");
}

#[test]
fn thumbnail_names_and_urls() {
    assert_eq!(
        thumbnail_file("Pokemon - Ruby & Sapphire: X?"),
        "Pokemon - Ruby _ Sapphire_ X_.png"
    );
    let snes = systems::by_key("SNES").unwrap();
    assert_eq!(
        libretro_name(snes),
        Some("Nintendo - Super Nintendo Entertainment System")
    );
    assert_eq!(
        thumbnail_url(
            THUMBNAILS_BASE,
            libretro_name(snes).unwrap(),
            ArtKind::Boxart,
            "Super Mario World (USA)"
        )
        .as_deref(),
        Some(
            "https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System/Named_Boxarts/Super%20Mario%20World%20(USA).png"
        )
    );
    assert_eq!(
        thumbnail_url(THUMBNAILS_BASE, "x", ArtKind::Hero, "y"),
        None,
        "libretro has no heroes"
    );
    assert_eq!(image_ext(b"\xff\xd8\xff\xe0"), Some("jpg"));
    assert_eq!(image_ext(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
    assert_eq!(image_ext(b"<html>"), None);
    let psx = systems::by_key("PlayStation").unwrap();
    assert_eq!(libretro_name(psx), Some("Sony - PlayStation"));
}
