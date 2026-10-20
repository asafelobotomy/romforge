use super::*;

#[test]
fn readmes_name_titles() {
    let text = text_words("The goal: insert the translated script of Phantasy Star 2 into the game.");
    assert_eq!(named_in(&key("Phantasy Star II (USA, Europe) (Rev A)"), &text), 3);
    assert_eq!(
        named_in(&key("Phantasy Star (USA, Europe)"), &text),
        0,
        "a 2 follows"
    );
    assert_eq!(named_in(&key("Star (Japan)"), &text), 0, "too short to say");
    let index = NameIndex::new([
        "Phantasy Star (Japan)",
        "Phantasy Star II (Japan)",
        "Star (Japan)",
    ]);
    assert_eq!(index.named(&text), vec![1]);
    let index = NameIndex::new(["Star (Japan)", "Phantasy Star II (Japan)"]);
    assert_eq!(index.named(&text), vec![1], "positions in the input");
}

#[test]
fn keys_normalize() {
    let k = key("Legend of Zelda, The - A Link to the Past (USA)");
    assert_eq!(k.main, ["legend", "of", "zelda"]);
    assert_eq!(k.sub, ["link", "to", "past"]);
    assert_eq!(key("The Legend of Zelda: A Link to the Past").main, k.main);
    assert_eq!(
        key("Final Fantasy VII (USA) (Disc 1)").hard,
        BTreeSet::from(["7".into()])
    );
    assert_eq!(key("Pokémon - Emerald Version").main, ["pokemon"]);
    assert_eq!(key("Ghouls & Ghosts").main, ["ghouls", "and", "ghosts"]);
    // A trailing 1 (or I) is the first game, named without it in DATs.
    assert_eq!(key("Final Fantasy I").main, ["final", "fantasy"]);
    assert_eq!(
        score(
            "Shin Megami Tensei 1 (English Addendum 1.0)",
            "Shin Megami Tensei (Japan)"
        ),
        1.0
    );
    assert_eq!(
        score("Shin Megami Tensei 1", "Shin Megami Tensei II (Japan)"),
        0.0
    );
    assert_eq!(key("I Am Setsuna").main, ["i", "am", "setsuna"]);
}

#[test]
fn number_titles_and_lone_letters() {
    assert_eq!(score("1942", "1942 (Japan, USA)"), 1.0);
    assert_eq!(score("1942", "1943 - The Battle of Midway (USA)"), 0.0);
    // A lone X is a letter, not 10.
    assert_eq!(score("Mega Man X (USA)", "Mega Man 10 (USA)"), 0.0);
    assert_eq!(score("Mega Man X", "Mega Man X (USA)"), 1.0);
    assert_eq!(key("X-Men (USA)").main, ["x", "men"]);
    assert_eq!(key("Final Fantasy VI (Japan)").hard, BTreeSet::from(["6".into()]));
}

#[test]
fn numerals_and_editions_veto() {
    assert_eq!(score("Street Fighter II (USA)", "Street Fighter III"), 0.0);
    assert_eq!(score("Street Fighter II", "Street Fighter II Turbo"), 0.0);
    assert_eq!(score("Street Fighter 2", "Street Fighter II (Japan)"), 1.0);
    assert_eq!(score("Dragon Quest III", "Dragon Quest I & II (Japan)"), 0.0);
    assert_eq!(score("Pokemon - Gold Version", "Pokemon - Silver Version"), 0.0);
}

#[test]
fn bands() {
    assert_eq!(
        score("Super Mario World (USA)", "Super Mario World (Europe) (Rev 1)"),
        1.0
    );
    let missing = score(
        "Dragon Quest III (English v2.0)[Addendum].sfc",
        "Dragon Quest III - Soshite Densetsu e... (Japan)",
    );
    assert_eq!(missing, 0.9);
    // "Ghouls'n" is "ghouls n": the same words.
    assert_eq!(score("Super Ghouls n Ghosts", "Super Ghouls'n Ghosts (USA)"), 1.0);
    // A typo stays below the exact bands.
    let typo = score("Super Ghoulss n Ghosts", "Super Ghouls'n Ghosts (USA)");
    assert_eq!(typo, 0.85, "every word, one with a typo");
    assert_eq!(
        score(
            "zelda link to the past",
            "Legend of Zelda, The - A Link to the Past (USA)"
        ),
        0.85
    );
    assert_eq!(
        score("Dragon Quest III", "Alien 3 (USA)"),
        0.0,
        "only the number in common"
    );
    assert!(
        score("Mario", "Super Mario World") < 0.85,
        "one word is not every word"
    );
    let other_sub = score(
        "Super Star Wars - Return of the Jedi",
        "Super Star Wars - The Empire Strikes Back",
    );
    assert!(other_sub > 0.0 && other_sub < 0.9, "{other_sub}");
    assert_eq!(score("Super Mario World", "Donkey Kong Country"), 0.0);
}

#[test]
fn header_titles() {
    assert!(header_fits(
        "DRAGONQUEST3",
        "Dragon Quest III - Soshite Densetsu e... (Japan)"
    ));
    assert!(header_fits("SUPER MARIOWORLD", "Super Mario World (USA)"));
    assert!(header_fits(
        "THE LEGEND OF ZELDA",
        "Legend of Zelda, The - A Link to the Past (USA)"
    ));
    assert!(header_fits(
        "SUPER EMPIRE STRIKES",
        "Super Empire Strikes Back (Japan)"
    ));
    // The real header of Return of the Jedi: both its words are there.
    assert!(header_fits(
        "SUPER JEDI",
        "Super Star Wars - Return of the Jedi (USA)"
    ));
    assert!(!header_fits("SUPER JEDI", "Super Mario World (USA)"));
    assert!(!header_fits("DRAGONQUEST3", "Dragon Quest I & II (Japan)"));
    assert!(!header_fits("MARIO", "Mario Paint (USA)"), "too short to tell");
    assert!(!header_fits("MEGAMAN 2", "Mega Man 20 (USA)"), "2 is not 20");
    assert!(!header_fits("FINAL FANTASY 3", "Final Fantasy 30th (Japan)"));
    assert!(
        header_fits("STREET FIGHTER", "Street Fighter II (USA)"),
        "cut before the number"
    );
}

/// The grouped index finds exactly what scoring every title would.
#[test]
fn index_matches_scoring_everything() {
    let titles: Vec<String> = [
        "Street Fighter II (USA)",
        "Street Fighter II Turbo (USA)",
        "Street Fighter Alpha 2 (USA)",
        "Super Mario World (USA)",
        "Super Mario World 2 - Yoshi's Island (USA)",
        "Super Street Fighter II (USA)",
        "1942 (Japan, USA)",
        "Final Fantasy III (USA)",
        "Final Fantasy VI (Japan)",
    ]
    .map(String::from)
    .to_vec();
    let idx = TitleIndex::new(titles.clone());
    for q in [
        "street fighter 2",
        "Super Mario World",
        "final fantasy 3",
        "1942",
        "super street fighter ii",
    ] {
        let mut brute: Vec<(String, f32)> = titles
            .iter()
            .map(|t| (t.clone(), score(q, t)))
            .filter(|(_, s)| *s > 0.0)
            .collect();
        brute.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        assert_eq!(idx.find(q, 50), brute, "{q}");
    }
}

#[test]
fn index_finds_by_shared_words() {
    let idx = TitleIndex::new(
        [
            "Dragon Quest III - Soshite Densetsu e... (Japan)",
            "Dragon Quest I & II (Japan)",
            "Dragon Quest V - Tenkuu no Hanayome (Japan)",
            "Super Mario World (USA)",
        ]
        .map(String::from),
    );
    let found = idx.find("Dragon Quest III (English v2.0)[Addendum]", 5);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "Dragon Quest III - Soshite Densetsu e... (Japan)");
}
