use super::*;

const LIST: &str = r#"scummvm (
	author "scummvm"
	version "2.9.0"
)

game (
	name "sky"
	title "Beneath a Steel Sky"
	extra "CD"
	language "en"
	platform "pc"
	sourcefile "sky"
	engine "sky"
	rom ( name "SKY.DNR" size 80 md5-5000 0123456789abcdef0123456789abcdef )
	rom ( name "sky.dsk" size -1 md5-5000 t:0123456789abcdef0123456789abcdef )
)

game (
	name "macgame"
	title "A Mac Game"
	extra ""
	language "en"
	platform "macintosh"
	sourcefile "x"
	engine "x"
	rom ( name "Game" size-rd 100 md5-5000 r:0123456789abcdef0123456789abcdef )
)
"#;

#[test]
fn parses_scummvm_s_list() {
    let e = parse(LIST).unwrap();
    assert_eq!(e.len(), 1, "the resource-fork entry is skipped");
    assert_eq!(e[0].code, "sky");
    assert_eq!(e[0].row_title(), "Beneath a Steel Sky (CD) (pc) (en)");
    assert_eq!(e[0].files[0].name, "sky.dnr");
    assert_eq!(e[0].files[0].size, Some(80));
    assert_eq!(e[0].files[1].size, None);
    assert!(e[0].files[1].tail && e[0].files[1].bytes == 5000);
    assert!(parse("clrmamepro ( name \"x\" )").is_err());
}

/// As the real dump has them: escaped quotes, wildcard names.
#[test]
fn escapes_and_wildcards() {
    let e = parse(concat!(
        "scummvm (\n)\n",
        "game (\n\tname \"bc\"\n\ttitle \"The Black Cauldron\"\n\textra \"1.1J 5.25\\\" Booter\"\n",
        "\trom ( name \"*\" size -1 md5-5000 0f69951170868481acebf831dd743b21 )\n)\n",
        "game (\n\tname \"bc\"\n\ttitle \"The Black Cauldron\"\n\textra \"2.00 \\\"Disk\\\"\"\n",
        "\trom ( name \"logdir\" size 381 md5-5000 0f69951170868481acebf831dd743b21 )\n",
        "\trom ( name \"MOV000.SMK\" size -1 md5-0  )\n)\n",
    ))
    .unwrap();
    assert_eq!(e.len(), 1, "the wildcard entry is skipped");
    assert_eq!(e[0].files[1].md5, "", "only has to be there");
    assert_eq!(e[0].extra, "2.00 \u{201d}Disk\u{201d}");
}

#[test]
fn partial_md5_of_head_and_tail() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("f");
    std::fs::write(&p, b"abcdef").unwrap();
    // md5("abc"), md5("def")
    assert_eq!(
        partial_md5(&p, 3, false).unwrap(),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        partial_md5(&p, 3, true).unwrap(),
        "4ed9407630eb1000c0f6b63842defa7d"
    );
    assert_eq!(
        partial_md5(&p, 5000, false).unwrap(),
        "e80b5017098950fc58aad83c8c14978e"
    );
}
