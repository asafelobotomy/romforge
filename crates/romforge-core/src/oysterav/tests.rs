use super::*;
use std::os::unix::fs::PermissionsExt;

fn fake(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join("oyst-cli");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

const T: Duration = Duration::from_millis(1);

#[test]
fn a_clean_answer_is_clean() {
    let d = tempfile::tempdir().unwrap();
    let cli = fake(
        d.path(),
        r#"echo '{"event":"progress"}'; echo '{"event":"result","clean":true,"findings":[],"pack_errors":[]}'"#,
    );
    let f = d.path().join("a");
    std::fs::write(&f, "x").unwrap();
    let v = scan_with(&cli, &[&f], T, 1);
    assert_eq!(v.state, State::Clean);
}

#[test]
fn a_threat_names_the_file_and_the_threat() {
    let d = tempfile::tempdir().unwrap();
    let cli = fake(
        d.path(),
        r#"echo '{"event":"result","clean":false,"findings":[{"path":"/g/a.zip","threat_name":"Bad.One"},{"path":"/g/b.zip","threat_name":"Bad.One"}],"pack_errors":[]}'; exit 1"#,
    );
    let v = scan_with(&cli, &[d.path()], T, 1);
    assert_eq!(v.state, State::Threat);
    assert_eq!(v.detail, "Bad.One");
    assert_eq!(v.findings.len(), 2);
    assert_eq!(v.findings[0].path, Path::new("/g/a.zip"));
}

#[test]
fn a_pack_error_or_no_answer_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let cli = fake(
        d.path(),
        r#"echo '{"event":"result","clean":false,"findings":[],"pack_errors":[{"error":"clamd is down"}]}'; exit 2"#,
    );
    let v = scan_with(&cli, &[d.path()], T, 1);
    assert_eq!((v.state, v.detail.as_str()), (State::Error, "clamd is down"));
    let cli = fake(d.path(), "echo 'no pack' >&2; exit 5");
    let v = scan_with(&cli, &[d.path()], T, 1);
    assert_eq!((v.state, v.detail.as_str()), (State::Error, "no pack"));
}

#[test]
fn busy_is_asked_again_then_given_up_on() {
    let d = tempfile::tempdir().unwrap();
    let count = d.path().join("n");
    let cli = fake(
        d.path(),
        &format!(
            r#"echo x >> {c}; if [ "$(wc -l < {c})" -lt 3 ]; then exit 3; fi
echo '{{"event":"result","clean":true,"findings":[],"pack_errors":[]}}'"#,
            c = count.display()
        ),
    );
    assert_eq!(scan_with(&cli, &[d.path()], T, 5).state, State::Clean);
    std::fs::remove_file(&count).unwrap();
    let busy = fake(d.path(), "exit 3");
    let v = scan_with(&busy, &[d.path()], T, 2);
    assert_eq!(v.state, State::Error);
}

#[test]
fn missing_things_are_errors_not_panics() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(
        scan_with(&d.path().join("none"), &[d.path()], T, 1).state,
        State::Error
    );
    let cli = fake(d.path(), "exit 0");
    assert_eq!(scan_with(&cli, &[Path::new("rel")], T, 1).state, State::Error);
    assert_eq!(
        scan_with(&cli, &[&d.path().join("gone")], T, 1).state,
        State::Error
    );
}
