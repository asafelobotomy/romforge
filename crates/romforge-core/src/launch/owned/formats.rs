//! Writing keys into emulators' config files, each in its own format, with
//! everything else in the file (other keys, comments, order) kept.

use crate::Error;

/// A config file's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// RetroArch: `key = "value"`.
    RetroArch,
    /// INI: `[Section]` then `Key = Value`; keys are written `[Section]Key`.
    Ini,
    /// Mednafen: `key value`.
    Spaced,
}

/// `text` with `keys` set and the keys in `remove` gone: a key's line is
/// replaced where it is, a key with no line is added (at its section's end
/// for INI).
pub fn merge(
    format: Format,
    text: &str,
    keys: &[(String, String)],
    remove: &[String],
) -> Result<String, Error> {
    for (k, v) in keys {
        if v.contains('\n') || (format == Format::RetroArch && v.contains('"')) {
            return Err(Error::Launch(format!(
                "{k}: can't write {v:?} into a config file"
            )));
        }
    }
    Ok(match format {
        Format::Ini => ini(text, keys, remove),
        _ => flat(format, text, keys, remove),
    })
}

fn flat(format: Format, text: &str, keys: &[(String, String)], remove: &[String]) -> String {
    let line = |k: &str, v: &str| match format {
        Format::RetroArch => format!("{k} = \"{v}\""),
        _ => format!("{k} {v}"),
    };
    let key_of = |l: &str| -> Option<String> {
        match format {
            Format::RetroArch => l.split_once('=').map(|(k, _)| k.trim().to_owned()),
            _ => l
                .split_whitespace()
                .next()
                .filter(|k| !k.starts_with(';'))
                .map(str::to_owned),
        }
    };
    let mut done = vec![false; keys.len()];
    let mut out = String::new();
    for l in text.lines() {
        match key_of(l).and_then(|k| keys.iter().position(|(key, _)| *key == k)) {
            Some(i) if !done[i] => {
                done[i] = true;
                out.push_str(&line(&keys[i].0, &keys[i].1));
            }
            Some(_) => continue,
            None if key_of(l).is_some_and(|k| remove.contains(&k)) => continue,
            None => out.push_str(l),
        }
        out.push('\n');
    }
    for (i, (k, v)) in keys.iter().enumerate() {
        if !done[i] {
            out.push_str(&line(k, v));
            out.push('\n');
        }
    }
    out
}

/// `[Section]Key` split.
fn split(key: &str) -> (&str, &str) {
    key.strip_prefix('[')
        .and_then(|k| k.split_once(']'))
        .unwrap_or(("", key))
}

fn ini(text: &str, keys: &[(String, String)], remove: &[String]) -> String {
    let keys: Vec<(&str, &str, &str)> = keys
        .iter()
        .map(|(k, v)| {
            let (s, k) = split(k);
            (s, k, v.as_str())
        })
        .collect();
    let remove: Vec<(&str, &str)> = remove.iter().map(|k| split(k)).collect();
    let mut done = vec![false; keys.len()];
    let mut out = String::new();
    let mut section = String::new();
    // Keys of `section` not written yet, at its end.
    // (before the blank lines that end it).
    let flush = |out: &mut String, section: &str, done: &mut Vec<bool>| {
        let blanks = out.len() - out.trim_end_matches('\n').len();
        let keep = blanks.saturating_sub(1);
        out.truncate(out.len() - keep);
        for (i, (s, k, v)) in keys.iter().enumerate() {
            if !done[i] && *s == section {
                done[i] = true;
                out.push_str(&format!("{k} = {v}\n"));
            }
        }
        out.push_str(&"\n".repeat(keep));
    };
    for l in text.lines() {
        let t = l.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            flush(&mut out, &section, &mut done);
            section = name.trim().to_owned();
        } else if let Some((k, _)) = t
            .split_once('=')
            .filter(|_| !t.starts_with(';') && !t.starts_with('#'))
        {
            let k = k.trim();
            if let Some(i) = keys.iter().position(|(s, key, _)| *s == section && *key == k) {
                if !done[i] {
                    done[i] = true;
                    out.push_str(&format!("{k} = {}\n", keys[i].2));
                }
                continue;
            }
            if remove.contains(&(section.as_str(), k)) {
                continue;
            }
        }
        out.push_str(l);
        out.push('\n');
    }
    flush(&mut out, &section, &mut done);
    // Sections the file doesn't have yet.
    for (i, (s, _, _)) in keys.iter().enumerate() {
        if !done[i] {
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push('\n');
            }
            if !s.is_empty() {
                out.push_str(&format!("[{s}]\n"));
            }
            flush(&mut out, s, &mut done);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn retroarch_keys_are_set_and_the_rest_kept() {
        let text =
            "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/old\"\nsystem_directory = \"/dup\"\n";
        let keys = kv(&[
            ("system_directory", "/rf/Bios/RetroArch"),
            ("config_save_on_exit", "false"),
        ]);
        assert_eq!(
            merge(Format::RetroArch, text, &keys, &[]).unwrap(),
            "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/rf/Bios/RetroArch\"\nconfig_save_on_exit = \"false\"\n"
        );
        assert!(merge(Format::RetroArch, "", &kv(&[("x", "a\"b")]), &[]).is_err());
        // A setting no longer set goes, so RetroArch's default applies.
        let gone = merge(Format::RetroArch, text, &[], &["video_driver".to_owned()]).unwrap();
        assert!(!gone.contains("video_driver"));
    }

    #[test]
    fn ini_keys_go_in_their_sections() {
        let text = "[Main]\nSettingsVersion = 3\n\n[BIOS]\nSearchDirectory = bios\nPathNTSCU = x.bin\n";
        let keys = kv(&[
            ("[BIOS]SearchDirectory", "/rf/Bios/duckstation"),
            ("[Main]SetupWizardIncomplete", "false"),
            ("[UI]SettingsVersion", "1"),
        ]);
        let out = merge(Format::Ini, text, &keys, &["[BIOS]PathNTSCU".to_owned()]).unwrap();
        assert_eq!(
            out,
            "[Main]\nSettingsVersion = 3\nSetupWizardIncomplete = false\n\n[BIOS]\nSearchDirectory = /rf/Bios/duckstation\n\n[UI]\nSettingsVersion = 1\n"
        );
        let fresh = merge(
            Format::Ini,
            "",
            &kv(&[("[ports.qt]gba.bios", "/b/gba_bios.bin")]),
            &[],
        )
        .unwrap();
        assert_eq!(fresh, "[ports.qt]\ngba.bios = /b/gba_bios.bin\n");
    }

    #[test]
    fn mednafen_lines_are_key_space_value() {
        let text = ";VERSION 1.32.1\npsx.bios_na scph5501.bin\nfilesys.path_firmware firmware\n";
        let out = merge(
            Format::Spaced,
            text,
            &kv(&[("filesys.path_firmware", "/rf/Bios/mednafen")]),
            &[],
        )
        .unwrap();
        assert_eq!(
            out,
            ";VERSION 1.32.1\npsx.bios_na scph5501.bin\nfilesys.path_firmware /rf/Bios/mednafen\n"
        );
    }
}
