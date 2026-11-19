//! DuckStation from Flathub, started the way romforge starts it, with a
//! stand-in `flatpak` that only lets the program see what it was granted
//! and a stand-in DuckStation that does what Flathub's build does at start
//! (`qthost.cpp`, 0.1-9482): it reads `$XDG_CONFIG_HOME/duckstation/
//! settings.ini` and resets every setting unless `[Main] SettingsVersion`
//! is 3. Not covered here: the real build doing so (see PLAN.md).
#![cfg(unix)]

use std::path::{Path, PathBuf};

use romforge_core::home::Home;
use romforge_core::launch::{Env, spawn};
use romforge_core::{Library, Scope};

mod common;
use common::launch::{fake, flatpak, script};

const ID: &str = "org.duckstation.DuckStation";

/// What Flathub's DuckStation does with its settings file at start; leaves
/// `kept` or `reset` in `$OUT` (and `denied` when the sandbox hides the
/// folder it was told to use).
const DUCKSTATION: &str = r#"
ini="$XDG_CONFIG_HOME/duckstation/settings.ini"
version=$(awk '/^\[Main\]/{m=1;next} /^\[/{m=0} m && /^SettingsVersion[ ]*=/{sub(/^[^=]*=[ ]*/,""); print; exit}' "$ini" 2>/dev/null)
if [ "$version" = 3 ]; then echo kept > "$OUT"; else printf '[Main]\nSettingsVersion = 3\n' > "$ini"; echo reset > "$OUT"; fi
"#;

/// `flatpak run [--filesystem=P]… --command=env ID K=V… CMD ARGS…`: runs
/// `CMD` (the stand-in) with `K=V`, and only if `$XDG_CONFIG_HOME` is under
/// a folder that was granted or the stand-in's own (home) one.
const FLATPAK: &str = r#"
[ "$1" = run ] || exit 2
shift
grants=""
while case "$1" in --filesystem=*) grants="$grants ${1#--filesystem=}"; true;; *) false;; esac; do shift; done
[ "$1" = --command=env ] && shift && [ "$1" = "$APP" ] && shift || exit 3
while case "$1" in *=*) export "$1"; true;; *) false;; esac; do shift; done
shift
for g in $grants; do case "$XDG_CONFIG_HOME/" in "$g"/*) exec "$STANDIN" "$@";; esac; done
echo denied > "$OUT"
"#;

struct Setup {
    _t: tempfile::TempDir,
    _f: common::launch::Fake,
    lib: Library,
    env: Env,
    home: Home,
    out: PathBuf,
    flatpak: PathBuf,
}

fn setup() -> Setup {
    let t = tempfile::tempdir().unwrap();
    let f = fake();
    flatpak(&f, ID);
    let active = f.root.join(format!("flatpak/{ID}/current/active"));
    std::fs::write(
        active.join("metadata"),
        format!("[Application]\nname={ID}\ncommand=duckstation-qt\n"),
    )
    .unwrap();
    let out = t.path().join("result.txt");
    let standin = t.path().join("duckstation-qt");
    script(&standin, DUCKSTATION);
    let flatpak = t.path().join("flatpak");
    script(
        &flatpak,
        &format!(
            "APP={ID}\nSTANDIN='{}'\nOUT='{}'\nexport OUT\n{FLATPAK}",
            standin.display(),
            out.display()
        ),
    );
    // PlayStation disc in the library (a track and its cue).
    let track = vec![7u8; 2352 * 4];
    let cue = b"FILE \"Disc (USA).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n".to_vec();
    let h = |b: &[u8]| romforge_core::hash::hash_stream(b, b.len() as u64).unwrap().raw;
    let rom = |name: &str, b: &[u8]| {
        let x = h(b);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    let dat = t.path().join("ps1.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Disc (USA)">{}{}</game></datafile>"#,
            rom("Disc (USA).bin", &track),
            rom("Disc (USA).cue", &cue)
        ),
    )
    .unwrap();
    let roms = t.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(roms.join("Disc (USA).bin"), &track).unwrap();
    std::fs::write(roms.join("Disc (USA).cue"), &cue).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();

    let home = Home::create(&t.path().join("parent")).unwrap();
    std::fs::create_dir_all(home.bios().join("duckstation")).unwrap();
    std::fs::write(home.bios().join("duckstation/scph5501.bin"), b"bios").unwrap();
    let mut env = f.env.clone();
    env.home = Some(home.clone());
    Setup {
        _t: t,
        _f: f,
        lib,
        env,
        home,
        out,
        flatpak,
    }
}

/// Start the game and wait for the stand-in to say what it did.
fn run(s: &Setup) -> String {
    let _ = std::fs::remove_file(&s.out);
    let mut spec = s.lib.launch_plan(&s.env, "Disc", Some("duckstation")).unwrap();
    assert_eq!(spec.program, Path::new("flatpak"), "{spec:?}");
    spec.program = s.flatpak.clone();
    spawn(&spec).unwrap();
    for _ in 0..200 {
        if let Ok(r) = std::fs::read_to_string(&s.out) {
            std::thread::sleep(std::time::Duration::from_millis(100)); // let it exit
            return r.trim().to_owned();
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the stand-in never answered: {spec:?}");
}

fn settings(s: &Setup) -> (PathBuf, String) {
    let p = s
        .home
        .emulator_config("duckstation")
        .join("duckstation/settings.ini");
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    (p, text)
}

/// The stand-in does what the fix was for: a file as 0.18.7 wrote it (no
/// `SettingsVersion`) is wiped. Without this, the tests below prove nothing.
#[test]
fn the_stand_in_resets_a_file_without_a_settings_version() {
    let s = setup();
    let (p, _) = settings(&s);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "[Main]\nSetupWizardIncomplete = false\n[BIOS]\nSearchDirectory = /b\n",
    )
    .unwrap();
    let standin = s._t.path().join("duckstation-qt");
    let out = std::process::Command::new(&standin)
        .env("XDG_CONFIG_HOME", s.home.emulator_config("duckstation"))
        .env("OUT", &s.out)
        .status()
        .unwrap();
    assert!(out.success());
    assert_eq!(std::fs::read_to_string(&s.out).unwrap().trim(), "reset");
    assert!(!std::fs::read_to_string(&p).unwrap().contains("SearchDirectory"));
}

#[test]
fn first_start_through_flatpak_keeps_romforge_s_settings() {
    let s = setup();
    let cfg = s.home.emulator_config("duckstation");
    let spec = s.lib.launch_plan(&s.env, "Disc", Some("duckstation")).unwrap();
    let a = &spec.args;
    // Where DuckStation reads: its XDG config folder, given through `env`
    // because Flatpak sets its own XDG folders over `--env=`, and granted.
    assert_eq!(a[0], "run", "{a:?}");
    assert!(
        a.contains(&format!("--filesystem={}", cfg.display()))
            || a.iter().any(|g| {
                g.strip_prefix("--filesystem=")
                    .is_some_and(|g| cfg.starts_with(g))
            }),
        "{a:?}"
    );
    let at = a.iter().position(|x| x == "--command=env").expect("through env");
    assert_eq!(a[at + 1], ID);
    assert_eq!(a[at + 2], format!("XDG_CONFIG_HOME={}", cfg.display()));
    assert_eq!(a[at + 3], "duckstation-qt");

    assert!(settings(&s).1.is_empty(), "a first start: no file yet");
    assert_eq!(run(&s), "kept");
    let (_, ini) = settings(&s);
    assert!(ini.contains("[Main]\nSettingsVersion = 3\n"), "{ini}");
    assert!(ini.contains("SetupWizardIncomplete = false"), "{ini}");
    let bios = s.home.bios().join("duckstation");
    assert!(
        ini.contains(&format!("SearchDirectory = {}", bios.display())),
        "{ini}"
    );
}

#[test]
fn a_file_from_an_earlier_romforge_is_kept_with_the_user_s_own_keys() {
    let s = setup();
    let (p, _) = settings(&s);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "[Main]\nSetupWizardIncomplete = false\nConfirmPowerOff = false\n[GPU]\nResolutionScale = 4\n",
    )
    .unwrap();
    assert_eq!(run(&s), "kept");
    let (_, ini) = settings(&s);
    assert!(
        ini.contains("ResolutionScale = 4"),
        "the user's setting survives: {ini}"
    );
    assert_eq!(ini.matches("SettingsVersion").count(), 1, "{ini}");
    // And the next start finds it as it left it.
    assert_eq!(run(&s), "kept");
}

#[test]
fn a_folder_the_sandbox_can_t_see_is_caught() {
    // The grant is what lets Flatpak's DuckStation see romforge's folder:
    // without it the stand-in says so rather than passing.
    let s = setup();
    let mut spec = s.lib.launch_plan(&s.env, "Disc", Some("duckstation")).unwrap();
    spec.args.retain(|a| !a.starts_with("--filesystem="));
    spec.program = s.flatpak.clone();
    let _ = std::fs::remove_file(&s.out);
    spawn(&spec).unwrap();
    for _ in 0..200 {
        if let Ok(r) = std::fs::read_to_string(&s.out) {
            assert_eq!(r.trim(), "denied");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("no answer");
}
