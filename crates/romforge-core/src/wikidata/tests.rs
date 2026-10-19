use super::*;

fn cand(qid: &str, names: &[&str], years: &[i32]) -> Candidate {
    Candidate {
        qid: qid.into(),
        label: names[0].into(),
        years: years.iter().copied().collect(),
        names: names.iter().map(|n| normalize(n)).collect(),
        work: None,
    }
}

#[test]
fn titles_normalize() {
    assert_eq!(
        normalize("Legend of Zelda, The - A Link to the Past (USA)"),
        "the legend of zelda a link to the past"
    );
    assert_eq!(
        normalize("The Legend of Zelda: A Link to the Past"),
        "the legend of zelda a link to the past"
    );
    assert_eq!(normalize("Ghouls 'n Ghosts & More"), "ghouls n ghosts and more");
}

#[test]
fn picks_one_or_says_why_not() {
    let c = vec![
        cand("Q1", &["Super Mario World"], &[1990, 1991]),
        cand("Q2", &["Doom"], &[1993]),
        cand("Q3", &["Doom"], &[2016]),
        cand(
            "Q4",
            &["The Legend of Zelda: A Link to the Past", "A Link to the Past"],
            &[1991],
        ),
    ];
    assert_eq!(pick("Super Mario World (USA)", None, &c).unwrap().qid, "Q1");
    assert_eq!(
        pick("Legend of Zelda, The - A Link to the Past (USA)", None, &c)
            .unwrap()
            .qid,
        "Q4"
    );
    assert_eq!(pick("Doom (USA)", Some(1993), &c).unwrap().qid, "Q2");
    assert_eq!(
        pick("Doom (USA)", None, &c).unwrap_err().len(),
        2,
        "ambiguous without a year"
    );
    assert!(pick("Unknown Game (USA)", None, &c).unwrap_err().is_empty());
}

#[test]
fn own_label_beats_an_alias() {
    let c = [
        cand(
            "Q2421908",
            &["Castlevania: Rondo of Blood", "Castlevania: Dracula X"],
            &[1993, 1995],
        ),
        cand("Q2607689", &["Castlevania: Dracula X"], &[1995]),
    ];
    assert_eq!(
        pick("Castlevania - Dracula X (USA)", None, &c).unwrap().qid,
        "Q2607689"
    );
}

#[test]
fn edition_tags_fall_back_to_the_game() {
    let c = [
        cand("Q1", &["Resident Evil: Director's Cut"], &[1997]),
        cand("Q2", &["Super Star Wars"], &[1992]),
    ];
    assert_eq!(
        pick("Resident Evil - Director's Cut - Dual Shock Ver. (USA)", None, &c)
            .unwrap()
            .qid,
        "Q1"
    );
    assert!(
        pick("Super Star Wars - The Empire Strikes Back (USA)", None, &c).is_err(),
        "a subtitle that names the game is kept"
    );
    assert_eq!(
        names_for("Resident Evil - Director's Cut - Dual Shock Ver. (USA)"),
        [
            "resident evil director s cut dual shock ver",
            "resident evil director s cut"
        ]
    );
    assert_eq!(
        names_for("Pokemon - Gold Version (USA)"),
        ["pokemon gold version"]
    );
}

#[test]
fn an_edition_inherits_what_it_lacks() {
    let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    let mut edition = BTreeMap::from([
        ("publisher".to_string(), set(&["Capcom"])),
        ("wikipedia".to_string(), set(&["Resident Evil (1996 video game)"])),
    ]);
    let game = BTreeMap::from([
        ("publisher".to_string(), set(&["Virgin"])),
        ("developer".to_string(), set(&["Capcom"])),
        ("series".to_string(), set(&["Resident Evil"])),
        ("series_qid".to_string(), set(&["Q1"])),
        ("series_ordinal".to_string(), set(&["1"])),
    ]);
    inherit(&mut edition, &game);
    assert_eq!(
        edition["publisher"],
        set(&["Capcom"]),
        "the edition's own value stays"
    );
    assert_eq!(edition["developer"], set(&["Capcom"]));
    assert_eq!(edition["series_ordinal"], set(&["1"]));

    let mut own_series = BTreeMap::from([("series".to_string(), set(&["Other"]))]);
    inherit(&mut own_series, &game);
    assert!(
        !own_series.contains_key("series_ordinal"),
        "a series comes whole or not at all"
    );
}

#[test]
fn accents_fold() {
    let c = [cand("Q1", &["Pokémon Emerald Version"], &[2004])];
    assert_eq!(
        pick("Pokemon - Emerald Version (USA, Europe)", None, &c)
            .unwrap()
            .qid,
        "Q1"
    );
}

#[test]
fn a_game_beats_its_own_edition() {
    let game = cand("Q1", &["Tomba!"], &[1997]);
    let mut edition = cand("Q2", &["Tomba!"], &[1998]);
    edition.work = Some("Q1".into());
    let other = cand("Q3", &["Tomba!"], &[1999]);
    assert_eq!(
        pick("Tomba! (USA)", None, &[game.clone(), edition.clone()])
            .unwrap()
            .qid,
        "Q1"
    );
    assert_eq!(
        pick("Tomba! (USA)", None, &[game, edition, other])
            .unwrap_err()
            .len(),
        2,
        "two games of that name stay ambiguous"
    );
}

#[test]
fn search_words_drop_stray_letters() {
    assert_eq!(search_words("ghouls n ghosts"), "ghouls ghosts");
    assert_eq!(search_words("final fantasy 6"), "final fantasy 6");
    assert_eq!(search_words("x"), "x");
}

#[test]
fn parses_sparql_json() {
    let body = br#"{"results":{"bindings":[{"g":{"type":"uri","value":"http://www.wikidata.org/entity/Q123"},"label":{"type":"literal","value":"Doom"}}]}}"#;
    let rows = parse_results(body).unwrap();
    assert_eq!(rows[0]["g"], "Q123");
    assert_eq!(rows[0]["label"], "Doom");
}
