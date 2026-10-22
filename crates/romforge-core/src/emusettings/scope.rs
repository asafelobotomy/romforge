//! Changing one value, by the scope a shell names.

use super::{EmulatorSettings, GLOBAL};

impl EmulatorSettings {
    /// Set `key` to `value` (`None`: unset it) for `scope`: `global` for
    /// every emulator, else the emulator of that name (any case). Checked
    /// when saved ([`super::save`]).
    pub fn set(&mut self, scope: &str, key: &str, value: Option<String>) {
        let values = if scope.eq_ignore_ascii_case(GLOBAL) {
            &mut self.global
        } else {
            self.emulators.entry(scope.to_ascii_lowercase()).or_default()
        };
        match value {
            Some(v) => values.insert(key.to_owned(), v),
            None => values.remove(key),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_or_one_emulator() {
        let mut s = EmulatorSettings::default();
        s.set("Global", "fullscreen", Some("true".into()));
        s.set("RetroArch", "shader", Some("crt".into()));
        assert_eq!(s.global.get("fullscreen").map(String::as_str), Some("true"));
        assert!(s.emulators["retroarch"].contains_key("shader"));
        s.set("retroarch", "shader", None);
        assert!(!s.emulators["retroarch"].contains_key("shader"));
    }
}
