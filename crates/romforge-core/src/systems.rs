//! The one table of systems. Seeded into the `systems` table on every open.
//!
//! Based on remustwo's `0002_systems.sql` (MIT). `dat_names` are the DAT
//! header names (No-Intro / Redump style, without trailing `(...)` groups)
//! used to guess a DAT's system on import. `preferred_hash` is informational
//! only: matching always needs size + SHA1 or size + MD5.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct System {
    pub id: i64,
    pub key: &'static str,
    pub name: &'static str,
    pub manufacturer: &'static str,
    pub generation: i64,
    pub release_year: i64,
    pub preferred_hash: &'static str,
    pub is_disc_based: bool,
    pub is_handheld: bool,
    pub dat_names: &'static [&'static str],
}

#[allow(clippy::too_many_arguments)]
const fn s(
    id: i64,
    key: &'static str,
    name: &'static str,
    manufacturer: &'static str,
    generation: i64,
    release_year: i64,
    preferred_hash: &'static str,
    disc: u8,
    handheld: u8,
    dat_names: &'static [&'static str],
) -> System {
    System {
        id,
        key,
        name,
        manufacturer,
        generation,
        release_year,
        preferred_hash,
        is_disc_based: disc == 1,
        is_handheld: handheld == 1,
        dat_names,
    }
}

#[rustfmt::skip]
pub const SYSTEMS: &[System] = &[
    s(1, "NES", "Nintendo Entertainment System", "Nintendo", 3, 1983, "CRC32", 0, 0, &["Nintendo - Nintendo Entertainment System"]),
    s(2, "SNES", "Super Nintendo Entertainment System", "Nintendo", 4, 1990, "CRC32", 0, 0, &["Nintendo - Super Nintendo Entertainment System"]),
    s(3, "N64", "Nintendo 64", "Nintendo", 5, 1996, "CRC32", 0, 0, &["Nintendo - Nintendo 64"]),
    s(4, "GameCube", "Nintendo GameCube", "Nintendo", 6, 2001, "MD5", 1, 0, &[]),
    s(5, "Wii", "Nintendo Wii", "Nintendo", 7, 2006, "MD5", 1, 0, &[]),
    s(6, "Game Boy", "Nintendo Game Boy", "Nintendo", 4, 1989, "CRC32", 0, 1, &["Nintendo - Game Boy"]),
    s(7, "Game Boy Color", "Nintendo Game Boy Color", "Nintendo", 5, 1998, "CRC32", 0, 1, &["Nintendo - Game Boy Color"]),
    s(8, "Game Boy Advance", "Nintendo Game Boy Advance", "Nintendo", 6, 2001, "CRC32", 0, 1, &["Nintendo - Game Boy Advance"]),
    s(9, "Nintendo DS", "Nintendo DS", "Nintendo", 7, 2004, "CRC32", 0, 1, &["Nintendo - Nintendo DS"]),
    s(10, "Genesis", "Sega Genesis / Mega Drive", "Sega", 4, 1988, "CRC32", 0, 0, &["Sega - Mega Drive - Genesis"]),
    s(11, "Master System", "Sega Master System", "Sega", 3, 1985, "CRC32", 0, 0, &["Sega - Master System - Mark III"]),
    s(12, "Saturn", "Sega Saturn", "Sega", 5, 1994, "MD5", 1, 0, &["Sega - Saturn"]),
    s(13, "Dreamcast", "Sega Dreamcast", "Sega", 6, 1998, "MD5", 1, 0, &["Sega - Dreamcast"]),
    s(14, "PlayStation", "Sony PlayStation", "Sony", 5, 1994, "MD5", 1, 0, &["Sony - PlayStation"]),
    s(15, "PlayStation 2", "Sony PlayStation 2", "Sony", 6, 2000, "MD5", 1, 0, &["Sony - PlayStation 2"]),
    s(16, "PSP", "PlayStation Portable", "Sony", 7, 2004, "MD5", 1, 1, &["Sony - PlayStation Portable"]),
    s(17, "Atari 2600", "Atari 2600", "Atari", 2, 1977, "CRC32", 0, 0, &["Atari - Atari 2600", "Atari - 2600"]),
    s(18, "Atari 7800", "Atari 7800 ProSystem", "Atari", 3, 1986, "CRC32", 0, 0, &["Atari - Atari 7800", "Atari - 7800"]),
    s(19, "Lynx", "Atari Lynx", "Atari", 4, 1989, "CRC32", 0, 1, &["Atari - Atari Lynx", "Atari - Lynx"]),
    s(20, "TurboGrafx-16", "TurboGrafx-16 / PC Engine", "NEC", 4, 1987, "CRC32", 0, 0, &["NEC - PC Engine - TurboGrafx-16"]),
    s(21, "TurboGrafx-CD", "TurboGrafx-CD / PC Engine CD", "NEC", 4, 1988, "MD5", 1, 0, &["NEC - PC Engine CD & TurboGrafx CD"]),
    s(22, "Neo Geo", "SNK Neo Geo", "SNK", 4, 1990, "CRC32", 0, 0, &[]),
    s(23, "Sega CD", "Sega CD / Mega CD", "Sega", 4, 1991, "MD5", 1, 0, &["Sega - Mega-CD - Sega CD"]),
    s(24, "Game Gear", "Sega Game Gear", "Sega", 4, 1990, "CRC32", 0, 1, &["Sega - Game Gear"]),
    s(25, "Sega 32X", "Sega 32X", "Sega", 5, 1994, "CRC32", 0, 0, &["Sega - 32X"]),
    s(26, "Atari Jaguar", "Atari Jaguar", "Atari", 5, 1993, "CRC32", 0, 0, &["Atari - Atari Jaguar", "Atari - Jaguar"]),
    s(27, "Neo Geo Pocket", "Neo Geo Pocket / Color", "SNK", 5, 1998, "CRC32", 0, 1, &["SNK - NeoGeo Pocket", "SNK - NeoGeo Pocket Color"]),
    s(28, "WonderSwan", "Bandai WonderSwan / Color", "Bandai", 5, 1999, "CRC32", 0, 1, &["Bandai - WonderSwan", "Bandai - WonderSwan Color"]),
    s(29, "Virtual Boy", "Nintendo Virtual Boy", "Nintendo", 5, 1995, "CRC32", 0, 1, &["Nintendo - Virtual Boy"]),
    s(30, "Nintendo 3DS", "Nintendo 3DS", "Nintendo", 8, 2011, "MD5", 1, 1, &["Nintendo - Nintendo 3DS"]),
    s(31, "Nintendo Switch", "Nintendo Switch", "Nintendo", 9, 2017, "SHA1", 1, 1, &[]),
    s(32, "PlayStation Vita", "Sony PlayStation Vita", "Sony", 8, 2011, "MD5", 0, 1, &[]),
    s(33, "Commodore 64", "Commodore 64", "Commodore", 2, 1982, "CRC32", 0, 0, &[]),
    s(34, "Amiga", "Commodore Amiga", "Commodore", 3, 1985, "CRC32", 0, 0, &[]),
    s(35, "ZX Spectrum", "Sinclair ZX Spectrum", "Sinclair", 2, 1982, "CRC32", 0, 0, &[]),
    s(36, "SuperGrafx", "NEC PC Engine SuperGrafx", "NEC", 4, 1989, "CRC32", 0, 0, &["NEC - PC Engine SuperGrafx"]),
    s(37, "Xbox", "Microsoft Xbox", "Microsoft", 6, 2001, "MD5", 1, 0, &[]),
    s(38, "Xbox 360", "Microsoft Xbox 360", "Microsoft", 7, 2005, "MD5", 1, 0, &[]),
    s(39, "Arcade", "Arcade / MAME", "Various", 0, 1970, "CRC32", 0, 0, &[]),
    s(40, "3DO", "3DO Interactive Multiplayer", "Panasonic", 5, 1993, "MD5", 1, 0, &[]),
    s(41, "Neo Geo CD", "SNK Neo Geo CD", "SNK", 5, 1994, "MD5", 1, 0, &[]),
    s(42, "FDS", "Nintendo Famicom Disk System", "Nintendo", 3, 1986, "CRC32", 0, 0, &["Nintendo - Family Computer Disk System"]),
    s(43, "Atari 5200", "Atari 5200", "Atari", 2, 1982, "CRC32", 0, 0, &["Atari - Atari 5200", "Atari - 5200"]),
    s(44, "Atari 8-bit", "Atari 8-bit Family", "Atari", 2, 1979, "CRC32", 0, 0, &[]),
    s(45, "Atari ST", "Atari ST", "Atari", 3, 1985, "CRC32", 0, 0, &[]),
    s(46, "ColecoVision", "ColecoVision", "Coleco", 2, 1982, "CRC32", 0, 0, &["Coleco - ColecoVision"]),
    s(47, "Intellivision", "Mattel Intellivision", "Mattel", 2, 1979, "CRC32", 0, 0, &["Mattel - Intellivision"]),
    s(48, "MSX", "MSX", "Microsoft", 3, 1983, "CRC32", 0, 0, &["Microsoft - MSX"]),
    s(49, "MSX2", "MSX2", "Microsoft", 3, 1985, "CRC32", 0, 0, &["Microsoft - MSX2"]),
    s(50, "PC-FX", "NEC PC-FX", "NEC", 5, 1994, "MD5", 1, 0, &[]),
    s(51, "CD-i", "Philips CD-i", "Philips", 4, 1991, "MD5", 1, 0, &[]),
    s(52, "CD32", "Commodore Amiga CD32", "Commodore", 5, 1993, "MD5", 1, 0, &[]),
    s(53, "Naomi", "Sega Naomi", "Sega", 6, 1998, "MD5", 1, 0, &[]),
    s(54, "SG-1000", "Sega SG-1000", "Sega", 3, 1983, "CRC32", 0, 0, &["Sega - SG-1000"]),
    s(55, "Atari Jaguar CD", "Atari Jaguar CD", "Atari", 5, 1995, "MD5", 1, 0, &[]),
    s(56, "Wii U", "Nintendo Wii U", "Nintendo", 8, 2012, "SHA1", 1, 0, &[]),
    s(57, "PlayStation 3", "Sony PlayStation 3", "Sony", 7, 2006, "SHA1", 1, 0, &[]),
    s(58, "Amstrad CPC", "Amstrad CPC", "Amstrad", 3, 1984, "CRC32", 0, 0, &[]),
    s(59, "Enterprise 128", "Enterprise 64/128", "Intelligent Software", 3, 1985, "CRC32", 0, 0, &[]),
    s(60, "ZX 81", "Sinclair ZX 81", "Sinclair", 2, 1981, "CRC32", 0, 0, &[]),
    s(61, "Videoton TVC", "Videoton TVC", "Videoton", 3, 1985, "CRC32", 0, 0, &[]),
    s(62, "Sega Pico", "Sega Pico", "Sega", 4, 1993, "CRC32", 0, 0, &["Sega - PICO"]),
    s(63, "VIC-20", "Commodore VIC-20", "Commodore", 2, 1981, "CRC32", 0, 0, &[]),
    s(64, "Odyssey2", "Magnavox Odyssey2 / Philips Videopac", "Magnavox", 2, 1978, "CRC32", 0, 0, &["Magnavox - Odyssey 2"]),
    s(65, "Supervision", "Watara Supervision", "Watara", 4, 1992, "CRC32", 0, 1, &["Watara - Supervision"]),
    s(66, "Pocket Challenge V2", "Benesse Pocket Challenge V2", "Benesse", 5, 1999, "CRC32", 0, 1, &[]),
    s(67, "PC-98", "NEC PC-98", "NEC", 3, 1982, "CRC32", 0, 0, &[]),
    s(68, "Interton VC 4000", "Interton VC 4000", "Interton", 2, 1978, "CRC32", 0, 0, &[]),
    s(69, "Arcadia 2001", "Emerson Arcadia 2001", "Emerson", 2, 1982, "CRC32", 0, 0, &[]),
    s(70, "Vectrex", "GCE Vectrex", "GCE", 2, 1982, "CRC32", 0, 0, &["GCE - Vectrex"]),
    s(71, "Pokemon Mini", "Nintendo Pokémon Mini", "Nintendo", 6, 2001, "CRC32", 0, 1, &["Nintendo - Pokemon Mini"]),
    s(72, "Channel F", "Fairchild Channel F", "Fairchild", 2, 1976, "CRC32", 0, 0, &["Fairchild - Channel F"]),
    s(73, "Super Cassette Vision", "Epoch Super Cassette Vision", "Epoch", 2, 1984, "CRC32", 0, 0, &[]),
    s(74, "GP32", "GamePark GP32", "GamePark", 6, 2001, "CRC32", 0, 1, &[]),
    s(75, "Game.com", "Tiger Game.com", "Tiger", 5, 1997, "CRC32", 0, 1, &[]),
    s(76, "Studio II", "RCA Studio II", "RCA", 2, 1977, "CRC32", 0, 0, &[]),
    s(77, "Atomiswave", "Sega Atomiswave", "Sammy", 6, 2003, "MD5", 0, 0, &[]),
    s(78, "Casio PV-1000", "Casio PV-1000", "Casio", 2, 1983, "CRC32", 0, 0, &[]),
    s(79, "Super A'Can", "Funtech Super A'Can", "Funtech", 4, 1995, "CRC32", 0, 0, &[]),
    s(80, "Casio Loopy", "Casio Loopy", "Casio", 5, 1995, "CRC32", 0, 0, &[]),
    s(81, "Sharp X1", "Sharp X1", "Sharp", 3, 1982, "CRC32", 0, 0, &[]),
    s(82, "X68000", "Sharp X68000", "Sharp", 4, 1987, "CRC32", 0, 0, &[]),
    s(83, "PC", "IBM PC Compatible", "IBM", 5, 1981, "MD5", 1, 0, &[]),
    s(84, "PlayStation 4", "Sony PlayStation 4", "Sony", 8, 2013, "SHA1", 1, 0, &[]),
    s(85, "Xbox One", "Microsoft Xbox One", "Microsoft", 8, 2013, "SHA1", 1, 0, &[]),
    s(86, "Mac", "Apple Macintosh", "Apple", 3, 1984, "MD5", 1, 0, &[]),
    s(87, "FM Towns", "Fujitsu FM Towns", "Fujitsu", 4, 1989, "MD5", 1, 0, &[]),
    s(88, "PC-88", "NEC PC-8801", "NEC", 3, 1981, "CRC32", 0, 0, &[]),
    s(89, "CDTV", "Commodore CDTV", "Commodore", 4, 1991, "MD5", 1, 0, &[]),
    s(90, "Amiga CD", "Amiga CD", "Commodore", 4, 1993, "MD5", 1, 0, &[]),
    s(91, "Archimedes", "Acorn Archimedes", "Acorn", 4, 1987, "MD5", 1, 0, &[]),
    s(92, "Triforce", "Sega Triforce", "Sega", 6, 2002, "MD5", 1, 0, &[]),
    s(93, "Chihiro", "Sega Chihiro", "Sega", 6, 2002, "MD5", 1, 0, &[]),
    s(94, "VIS", "Tandy VIS", "Tandy", 5, 1992, "MD5", 1, 0, &[]),
    s(95, "Lindbergh", "Sega Lindbergh", "Sega", 7, 2005, "MD5", 1, 0, &[]),
    s(96, "Palm OS", "Palm OS", "Palm", 6, 1996, "CRC32", 0, 1, &[]),
    s(97, "Playdia", "Bandai Playdia", "Bandai", 5, 1994, "MD5", 1, 0, &[]),
    s(98, "Apple II", "Apple II / IIe / IIc / IIGS", "Apple", 2, 1977, "CRC32", 0, 0, &[]),
    s(99, "BBC Micro", "Acorn BBC Micro / Electron", "Acorn", 3, 1981, "CRC32", 0, 0, &[]),
    s(100, "Commodore 16", "Commodore 16 / Plus/4", "Commodore", 3, 1984, "CRC32", 0, 0, &[]),
    s(101, "Mega Duck", "Mega Duck / Cougar Boy", "Welback", 4, 1993, "CRC32", 0, 1, &["Welback - Mega Duck"]),
    s(102, "Microvision", "Milton Bradley Microvision", "Milton Bradley", 1, 1979, "CRC32", 0, 1, &[]),
    s(103, "Nuon", "VM Labs Nuon", "VM Labs", 6, 1999, "MD5", 1, 0, &[]),
    s(104, "V.Smile", "VTech V.Smile", "VTech", 6, 2004, "CRC32", 0, 0, &[]),
    s(105, "Leapster", "LeapFrog Leapster", "LeapFrog", 6, 2003, "CRC32", 0, 1, &[]),
    s(106, "LeapPad", "LeapFrog LeapPad", "LeapFrog", 5, 1999, "CRC32", 0, 0, &[]),
    s(107, "Beena", "Sega Beena", "Sega", 7, 2005, "CRC32", 0, 0, &[]),
    s(108, "CreatiVision", "VTech CreatiVision", "VTech", 2, 1981, "CRC32", 0, 0, &[]),
    s(109, "Picno", "Konami Picno", "Konami", 4, 1993, "CRC32", 0, 0, &[]),
    s(110, "Game Master", "Hartung Game Master", "Hartung", 4, 1990, "CRC32", 0, 1, &[]),
    s(111, "Symbian", "Mobile Symbian", "Nokia", 7, 2000, "CRC32", 0, 1, &[]),
    s(112, "Adventure Vision", "Entex Adventure Vision", "Entex", 2, 1982, "CRC32", 0, 0, &[]),
];

/// Find a system by its key (case-insensitive), e.g. `NES` or `game boy`.
pub fn by_key(key: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.key.eq_ignore_ascii_case(key.trim()))
}

pub fn by_id(id: i64) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.id == id)
}

/// Guess a system from a DAT header name such as
/// `Nintendo - Nintendo Entertainment System (Headered) (Parent-Clone)`.
/// Trailing `(...)` groups are ignored and the rest must match exactly, so
/// `Nintendo - Game Boy Color` never matches `Nintendo - Game Boy`.
pub fn from_dat_name(dat_name: &str) -> Option<&'static System> {
    let base = strip_paren_suffixes(dat_name);
    SYSTEMS
        .iter()
        .find(|s| s.dat_names.iter().any(|n| n.eq_ignore_ascii_case(base)))
}

fn strip_paren_suffixes(mut s: &str) -> &str {
    loop {
        s = s.trim_end();
        if s.ends_with(')')
            && let Some(open) = s.rfind(" (")
        {
            s = &s[..open];
            continue;
        }
        return s;
    }
}

#[cfg(test)]
mod tests {
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
    fn by_key_is_case_insensitive() {
        assert_eq!(by_key("snes").map(|s| s.id), Some(2));
        assert!(by_key("nope").is_none());
    }
}
