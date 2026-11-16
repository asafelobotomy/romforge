//! A download is looked at before it is put in place (own test binary: it sets an environment variable).

use std::os::unix::fs::PermissionsExt;

use romforge_core::oysterav::check_download;

#[test]
fn a_download_is_installed_only_when_oysterav_says_clean() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("core.so");
    std::fs::write(&file, "x").unwrap();
    let cli = d.path().join("oyst");
    let set = |body: &str| {
        std::fs::write(&cli, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    // SAFETY: the only test in this binary, so no other thread reads the environment.
    unsafe { std::env::set_var("ROMFORGE_OYST_CLI", &cli) };

    set(r#"echo '{"event":"result","clean":true,"findings":[],"pack_errors":[]}'"#);
    assert!(check_download(&file, "core.so").is_ok());

    set(
        r#"echo '{"event":"result","clean":false,"findings":[{"path":"x","threat_name":"Bad.One"}],"pack_errors":[]}'; exit 1"#,
    );
    let e = check_download(&file, "core.so").unwrap_err().to_string();
    assert!(e.contains("Bad.One") && e.contains("not installed"), "{e}");

    set("echo broken >&2; exit 2");
    let e = check_download(&file, "core.so").unwrap_err().to_string();
    assert!(e.contains("could not be checked") && e.contains("broken"), "{e}");
}
