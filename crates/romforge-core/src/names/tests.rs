use super::*;

#[test]
fn nointro_and_redump() {
    let n = parse("Legend of Zelda, The - A Link to the Past (USA).sfc");
    assert_eq!(n.convention, Convention::NoIntro);
    assert_eq!(n.title, "Legend of Zelda, The - A Link to the Past");
    assert_eq!(n.regions, ["USA"]);
    assert_eq!(n.region_codes, ["US"]);

    let n = parse("Streets of Rage (World) (En,Ja)");
    assert_eq!(n.regions, ["World"]);
    assert_eq!(n.languages, ["En", "Ja"]);

    let n = parse("Super Mario World (USA) (Rev 1) [b]");
    assert_eq!(n.version.as_deref(), Some("Rev 1"));
    assert_eq!(n.revision(), [1]);
    assert!(n.bad_dump);

    let n = parse("Final Fantasy VII (USA) (Disc 2)");
    assert_eq!(n.disc.as_deref(), Some("2"));

    let n = parse("Silent Hill (USA) (Beta) (1998-12-01)");
    assert!(!n.is_retail(), "{n:?}");
    assert!(!parse("101 Dalmatas (USA) (Pirate)").is_retail());
    let n = parse("16 BIT XMAS 2011 - Christmas Craze (World) (Aftermarket) (Unl)");
    assert!(n.is_retail() && n.unlicensed, "{n:?}");
}

#[test]
fn goodtools_and_tosec() {
    let n = parse("Dragon Quest III (J) [T+Eng2.0_Addendum].smc");
    assert_eq!(n.convention, Convention::GoodTools);
    assert_eq!(n.title, "Dragon Quest III");
    assert_eq!(n.region_codes, ["JP"]);
    assert_eq!(n.translation.as_deref(), Some("Eng2.0_Addendum"));

    let n = parse("Street Fighter II (E) [h1C]");
    assert_eq!(n.hack.as_deref(), Some("h1C"));

    let n = parse("Super Mario World (U) [!]");
    assert!(n.verified_dump);
    assert_eq!(n.region_codes, ["US"]);

    let n = parse("Dragon Quest III (1996)(Enix)(JP)[tr en Addendum]");
    assert_eq!(n.convention, Convention::Tosec);
    assert_eq!(n.title, "Dragon Quest III");
    assert_eq!(n.translation.as_deref(), Some("en Addendum"));
}

#[test]
fn loose_names() {
    let n = parse("Dragon Quest III (English v2.0)[Addendum].sfc");
    assert_eq!(n.convention, Convention::Loose);
    assert_eq!(n.title, "Dragon Quest III");
    assert_eq!(n.translation.as_deref(), Some("English v2.0 Addendum"));

    let n = parse("Super Metroid (Hack) (Redesign)");
    assert_eq!(n.title, "Super Metroid");
    assert!(n.hack.is_some());

    let n = parse("Post Mortem (Ukraine) (Disc 2)");
    assert_eq!(n.regions, ["Ukraine"], "{n:?}");
    assert_eq!(n.disc.as_deref(), Some("2"));
    assert_eq!(parse("ps-22j(v)").title, "ps-22j");
    let n = parse("Ancient Chozo [Hack by Albert V.]");
    assert_eq!(
        (n.title.as_str(), n.hack.as_deref()),
        ("Ancient Chozo", Some("Hack by Albert V."))
    );

    let n = parse("zelda3.smc");
    assert_eq!(n.title, "zelda3");
    assert_eq!(n.translation, None);
}

#[test]
fn extensions_only_when_they_look_like_one() {
    assert_eq!(without_extension("Game (USA).sfc"), "Game (USA)");
    assert_eq!(
        without_extension("Final Fantasy VII (USA) (v1.1)"),
        "Final Fantasy VII (USA) (v1.1)"
    );
    assert_eq!(without_extension("Doom 1.9"), "Doom 1.9");
}
