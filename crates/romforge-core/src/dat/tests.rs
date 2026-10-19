use super::*;

const DAT: &str = r#"<?xml version="1.0"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
  <header>
<name>Nintendo - Nintendo Entertainment System (Headerless)</name>
<description>Test &amp; more</description>
<version>20240101</version>
<clrmamepro header="No-Intro_NES.xml"/>
  </header>
  <game name="A (USA)">
<description>A (USA)</description>
<rom name="A (USA).nes" size="1024" crc="ABCDEF01" md5="00112233445566778899AABBCCDDEEFF" sha1="0123456789abcdef0123456789abcdef01234567" status="verified"/>
  </game>
  <machine name="b"><rom name="b.bin" size="0x10" crc="00000001"/></machine>
</datafile>"#;

#[test]
fn parses_header_games_and_roms() {
    let dat = parse(DAT.as_bytes()).unwrap();
    assert_eq!(dat.name, "Nintendo - Nintendo Entertainment System (Headerless)");
    assert_eq!(dat.description.as_deref(), Some("Test & more"));
    assert_eq!(dat.version.as_deref(), Some("20240101"));
    assert_eq!(dat.header_hint.as_deref(), Some("No-Intro_NES.xml"));
    assert_eq!(dat.games.len(), 2);
    let rom = &dat.games[0].roms[0];
    assert_eq!(rom.size, Some(1024));
    assert_eq!(rom.crc32.as_deref(), Some("abcdef01"));
    assert_eq!(rom.md5.as_deref(), Some("00112233445566778899aabbccddeeff"));
    assert_eq!(rom.status.as_deref(), Some("verified"));
    assert_eq!(dat.games[1].roms[0].size, Some(16));
    assert_eq!(dat.games[1].roms[0].sha1, None);
}

#[test]
fn rejects_non_dat() {
    assert!(parse("<html></html>".as_bytes()).is_err());
    assert!(parse_any(b"<!DOCTYPE html><html></html>").is_err());
    let err = parse_any(b"# romforge\n\nCatalog (and more)")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a DAT"), "{err}");
    assert!(parse_any(b"clrmamepro ( name \"x\" ").is_err());
}

const CMP: &str = r#"clrmamepro (
	name "Sony - PlayStation - BIOS Images"
	description "Sony - PlayStation - BIOS Images (24) (2016-10-21)"
	version 2016-10-21
	author "MESS & redump.org"
)

game (
	name "ps-10j"
	description "SCPH-1000/DTL-H1000 (Version 1.0 J)"
	rom ( name ps-22j(v).bin size 524288 crc 3b601fc8 md5 239665B1a3dade1b5a52c06338011044 sha1 343883a7b555646da8cee54aadd2795b6e7dd070 )
)
game (
	name "Super Mario Bros. (World)"
	region "World"
	rom ( name "Super Mario Bros. (World).nes" size 40976 crc 393A432F flags baddump )
	rom ( name "second (rom).bin" size 0x10 )
)
"#;

#[test]
fn parses_clrmamepro_text() {
    let dat = parse_any(CMP.as_bytes()).unwrap();
    assert_eq!(dat.name, "Sony - PlayStation - BIOS Images");
    assert_eq!(dat.version.as_deref(), Some("2016-10-21"));
    assert_eq!(dat.author.as_deref(), Some("MESS & redump.org"));
    assert_eq!(dat.games.len(), 2);
    let bios = &dat.games[0];
    assert_eq!(
        bios.description.as_deref(),
        Some("SCPH-1000/DTL-H1000 (Version 1.0 J)")
    );
    assert_eq!(bios.roms[0].name, "ps-22j(v).bin");
    assert_eq!(
        bios.roms[0].md5.as_deref(),
        Some("239665b1a3dade1b5a52c06338011044")
    );
    let smb = &dat.games[1];
    assert_eq!(smb.roms.len(), 2);
    assert_eq!(smb.roms[0].name, "Super Mario Bros. (World).nes");
    assert_eq!(smb.roms[0].crc32.as_deref(), Some("393a432f"));
    assert_eq!(smb.roms[0].status.as_deref(), Some("baddump"));
    assert_eq!(smb.roms[1].size, Some(16));
}

#[test]
fn parses_metadata_records() {
    let text = r#"clrmamepro ( name "x" )
game (
	comment "Super Mario World (USA)"
	releaseyear "1991"
	rom ( crc B19ED489 )
)
game (
	name "Silent Hill (USA)"
	serial "SLUS-00707"
	developer "KCET"
	rom ( serial "SLUS-00707" name "Silent Hill (USA).cue" )
)
game (
	comment "2 Games in 1"
	developer ""Natsume", Vicarious Visions"
	rom ( crc 9FF018F9 )
)
game (
	comment "EarthBound (USA)"
	developer "Ape", "HAL Laboratory"
)"#;
    let r = parse_cmp_records(text).unwrap();
    assert_eq!(r.len(), 4);
    // Lists written as several quoted strings come out plain.
    assert_eq!(
        r[2].fields,
        [("developer".to_owned(), "Natsume, Vicarious Visions".to_owned())]
    );
    assert_eq!(
        r[3].fields,
        [("developer".to_owned(), "Ape, HAL Laboratory".to_owned())]
    );
    assert_eq!(r[2].crcs, ["9ff018f9"]);
    assert_eq!(r[0].title.as_deref(), Some("Super Mario World (USA)"));
    assert_eq!(r[0].fields, [("releaseyear".to_owned(), "1991".to_owned())]);
    assert_eq!(r[0].crcs, ["b19ed489"]);
    assert_eq!(r[1].title.as_deref(), Some("Silent Hill (USA)"));
    assert!(r[1].fields.contains(&("developer".into(), "KCET".into())));
    assert_eq!(r[1].serials, ["SLUS-00707"]);
}

/// Parent, region, Redump category and serials, in both formats.
#[test]
fn keeps_parent_region_category_and_serials() {
    let xml = r#"<datafile><header><name>Sony - PlayStation</name></header>
  <game name="Silent Hill (USA)">
<category>Games</category>
<serial>SLUS-00707, SLUS-00707GHA</serial>
<description>Silent Hill (USA)</description>
<rom name="Silent Hill (USA).cue" size="1" crc="00000001"/>
  </game>
  <game name="Silent Hill (Europe)" cloneof="Silent Hill (USA)"><rom name="x" size="1"/></game>
</datafile>"#;
    let dat = parse_any(xml.as_bytes()).unwrap();
    let sh = &dat.games[0];
    assert_eq!(sh.category.as_deref(), Some("Games"));
    assert_eq!(sh.serials, ["SLUS-00707", "SLUS-00707GHA"]);
    assert_eq!(sh.description.as_deref(), Some("Silent Hill (USA)"));
    assert_eq!(dat.games[1].cloneof.as_deref(), Some("Silent Hill (USA)"));

    let cmp = r#"clrmamepro ( name "Nintendo - Super Nintendo Entertainment System" )
game (
	name "Dragon Quest III - Soshite Densetsu e... (Japan)"
	region "Japan"
	serial "SHVC-AQ3J-JPN"
	rom ( name "x.sfc" size 4194304 crc 13836BD2 )
)
"#;
    let dat = parse_any(cmp.as_bytes()).unwrap();
    assert_eq!(dat.games[0].region.as_deref(), Some("Japan"));
    assert_eq!(dat.games[0].serials, ["SHVC-AQ3J-JPN"]);
}

#[test]
fn bare_ampersands_are_read_as_text() {
    let xml = r#"<datafile><header><name>IBM - PC compatible</name></header>
  <game name="Hidden &amp; Dangerous (Europe)"><serial>H&D/CDCRYST/E</serial><rom name="x" size="1"/></game>
</datafile>"#;
    let dat = parse_any(xml.as_bytes()).unwrap();
    assert_eq!(dat.games[0].name, "Hidden & Dangerous (Europe)");
    assert_eq!(dat.games[0].serials, ["H&D/CDCRYST/E"]);
}

#[test]
fn serial_keys_meet() {
    assert_eq!(serial_key("SLUS-00067"), "SLUS00067");
    assert_eq!(serial_key("SLUS_000.67"), "SLUS00067", "as a PS1 disc boots it");
    assert_eq!(serial_key("SCPS 45196"), "SCPS45196");
    assert_eq!(serial_key("shvc-aq3j-jpn"), "SHVCAQ3JJPN");
}

#[test]
fn xml_still_detected() {
    assert_eq!(parse_any(DAT.as_bytes()).unwrap().games.len(), 2);
}
