//! Tidying RetroBIOS's profiles after reading them.
//!
//! A profile often lists one file several times: once per version the
//! emulator's code accepts (DOSBox's `MT32_CONTROL.ROM`, one hash each), or
//! once with hashes and once without (blueMSX's `MSX.rom`). Those become
//! one file, so a check shows one line for it, not one "accepted" and the
//! rest "wrong hash".
//!
//! A file a profile lists without a hash (mGBA's `gba_bios.bin`) borrows,
//! as *known* hashes, the ones other profiles give a file of the same name
//! for the same system. A dump matching one is accepted (house rule 8:
//! size + SHA1 or MD5 still decide), and any other is still only present,
//! never a wrong hash: the emulator itself checks none.

use std::collections::HashMap;

use super::FileReq;
use super::data::Profile;

/// Merge each profile's repeated files and share known hashes.
pub(super) fn tidy(profiles: &mut [Profile]) {
    for p in profiles.iter_mut() {
        p.files = merged(std::mem::take(&mut p.files));
    }
    share(profiles);
}

/// What makes two entries of one profile the same file: where it goes, for
/// which system, and how it counts.
fn same(a: &FileReq, b: &FileReq) -> bool {
    a.place() == b.place()
        && a.standalone_path == b.standalone_path
        && a.system == b.system
        && a.regions == b.regions
        && a.variant_group == b.variant_group
        && !a.archive
        && !b.archive
        && (a.size.is_none() || b.size.is_none() || a.size == b.size)
}

fn hashed(f: &FileReq) -> bool {
    !f.sha1.is_empty() || !f.md5.is_empty()
}

fn add(to: &mut Vec<String>, from: &[String]) {
    for h in from {
        if !to.contains(h) {
            to.push(h.clone());
        }
    }
}

/// `files` with repeats folded into the first: hashes joined (a repeat
/// with none makes them known hashes only), required if any is, a size
/// only where all give it.
fn merged(files: Vec<FileReq>) -> Vec<FileReq> {
    let mut out: Vec<FileReq> = Vec::with_capacity(files.len());
    for f in files {
        let Some(m) = out.iter_mut().find(|m| same(m, &f)) else {
            out.push(f);
            continue;
        };
        if hashed(m) != hashed(&f) || !m.known_sha1.is_empty() || !m.known_md5.is_empty() {
            // One of them takes any dump: keep that, and the hashes as known.
            let (sha1, md5) = (std::mem::take(&mut m.sha1), std::mem::take(&mut m.md5));
            add(&mut m.known_sha1, &sha1);
            add(&mut m.known_md5, &md5);
            add(&mut m.known_sha1, &f.sha1);
            add(&mut m.known_md5, &f.md5);
        } else {
            add(&mut m.sha1, &f.sha1);
            add(&mut m.md5, &f.md5);
        }
        if m.size != f.size {
            m.size = None;
        }
        m.required |= f.required;
        m.hle &= f.hle;
    }
    out
}

/// The RetroBIOS systems a file is for.
fn systems<'a>(p: &'a Profile, f: &'a FileReq) -> Vec<&'a str> {
    match &f.system {
        Some(s) => vec![s.as_str()],
        None => p.systems.iter().map(String::as_str).collect(),
    }
}

/// Give each file without a hash the hashes other profiles list for its
/// name on one of its systems, where the sizes agree.
fn share(profiles: &mut [Profile]) {
    type Hashes = (Option<u64>, Vec<String>, Vec<String>);
    let mut known: HashMap<(String, String), Vec<Hashes>> = HashMap::new();
    for p in profiles.iter() {
        for f in p.files.iter().filter(|f| hashed(f) && !f.archive) {
            for sys in systems(p, f) {
                known
                    .entry((sys.to_owned(), f.name.to_ascii_lowercase()))
                    .or_default()
                    .push((f.size, f.sha1.clone(), f.md5.clone()));
            }
        }
    }
    for p in profiles.iter_mut() {
        let ids: Vec<String> = p.systems.clone();
        for f in p.files.iter_mut().filter(|f| !hashed(f) && !f.archive) {
            let name = f.name.to_ascii_lowercase();
            let sys: Vec<&str> = match &f.system {
                Some(s) => vec![s.as_str()],
                None => ids.iter().map(String::as_str).collect(),
            };
            let mut sha1 = Vec::new();
            let mut md5 = Vec::new();
            for s in sys {
                let Some(list) = known.get(&(s.to_owned(), name.clone())) else {
                    continue;
                };
                for (size, s1, m5) in list {
                    if f.size.is_none() || size.is_none() || f.size == *size {
                        add(&mut sha1, s1);
                        add(&mut md5, m5);
                    }
                }
            }
            add(&mut f.known_sha1, &sha1);
            add(&mut f.known_md5, &md5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::data::parse;

    #[test]
    fn repeats_are_one_file_and_hashless_files_borrow_known_hashes() {
        let doc = serde_json::json!({"items": [
            {"id": "dosbox", "profile": {"systems": ["dos"], "files": [
                {"name": "MT32_CONTROL.ROM", "size": 4, "sha1": "1111111111111111111111111111111111111111"},
                {"name": "MT32_CONTROL.ROM", "size": 4, "sha1": "2222222222222222222222222222222222222222"},
                {"name": "MT32_CONTROL.ROM", "size": 8, "sha1": "3333333333333333333333333333333333333333"}
            ]}},
            {"id": "bluemsx", "profile": {"systems": ["msx"], "files": [
                {"name": "MSX.rom", "required": true, "size": 4},
                {"name": "MSX.rom", "size": 4, "md5": "44444444444444444444444444444444"}
            ]}},
            {"id": "mgba", "profile": {"systems": ["nintendo-gba"], "files": [
                {"name": "gba_bios.bin", "size": 4},
                {"name": "kanji.rom", "size": 4}
            ]}},
            {"id": "other", "profile": {"systems": ["nintendo-gba", "fm7"], "files": [
                {"name": "GBA_BIOS.BIN", "size": 4, "md5": "55555555555555555555555555555555"},
                {"name": "kanji.rom", "size": 4, "md5": "66666666666666666666666666666666", "system": "fm7"}
            ]}}
        ]})
        .to_string();
        let (profiles, _) = parse(doc.as_bytes()).unwrap();
        let dosbox = &profiles[0].files;
        assert_eq!(dosbox.len(), 2, "another size stays its own file");
        assert_eq!(dosbox[0].sha1.len(), 2);

        let msx = &profiles[1].files;
        assert_eq!(msx.len(), 1);
        assert!(msx[0].required && msx[0].md5.is_empty());
        assert_eq!(msx[0].known_md5, ["44444444444444444444444444444444"]);

        let mgba = &profiles[2].files;
        assert_eq!(
            mgba[0].known_md5,
            ["55555555555555555555555555555555"],
            "any case"
        );
        assert!(mgba[1].known_md5.is_empty(), "another system's kanji.rom");
    }
}
