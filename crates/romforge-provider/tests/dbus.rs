//! `org.uq.Launcher1.Provider` on a private session bus, as uQ's launcher
//! bridge calls it: Search rows from a real library (the repository's
//! fixtures, scanned read-only), input checks on Activate, a game started
//! through a stand-in emulator and one game at a time, no library at all,
//! and bus activation of the real binary from the packaged service file
//! through to its idle exit. Skipped when `dbus-daemon` is not installed.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use romforge_core::launch::UserEmulator;
use romforge_core::{Library, Scope};
use romforge_provider::{BUS_NAME, Games, PATH, Provider};
use tokio::sync::Notify;

type Row = (String, String, String, String, String, f64);

#[zbus::proxy(
    interface = "org.uq.Launcher1.Provider",
    default_service = "org.romforge.Provider",
    default_path = "/org/uq/Launcher1"
)]
trait Launcher {
    fn search(&self, query: &str) -> zbus::Result<Vec<Row>>;
    fn activate(&self, id: &str, action: &str) -> zbus::Result<()>;
}

/// A `dbus-daemon` of our own, stopped on drop.
struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start(dir: &Path, env: &[(&str, &Path)]) -> Option<Self> {
        let services = dir.join("services");
        std::fs::create_dir_all(&services).unwrap();
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <servicedir>{}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                dir.join("bus").display(),
                services.display()
            ),
        )
        .unwrap();
        let mut cmd = Command::new("dbus-daemon");
        cmd.arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => panic!("dbus-daemon: {e}"),
        };
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("unix:"), "dbus-daemon printed {line:?}");
        Some(Self {
            child,
            address: line.trim().to_string(),
        })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// A library on disk with the fixture NES games, and a stand-in NES
/// emulator that writes its arguments to `ran` and runs `seconds`.
fn library(dir: &Path, ran: &Path, seconds: u32) -> PathBuf {
    let db = dir.join("romforge.db");
    let mut lib = Library::open(&db).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/nes"), Scope::All).unwrap();
    let program = dir.join("nesemu");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}.part'\nmv '{0}.part' '{0}'\nsleep {seconds}\n",
            ran.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    lib.add_emulator(&UserEmulator {
        name: "nesemu".into(),
        system: "NES".into(),
        program: program.display().to_string(),
        args: vec!["{rom}".into()],
        archives: false,
    })
    .unwrap();
    db
}

async fn connect(address: &str) -> zbus::Connection {
    zbus::connection::Builder::address(address).build().await.unwrap()
}

async fn serve(address: &str, db: PathBuf) -> zbus::Connection {
    let provider = Provider {
        games: Arc::new(Games::new(db)),
        touched: Arc::new(Notify::new()),
    };
    zbus::connection::Builder::address(address)
        .name(BUS_NAME)
        .serve_at(PATH, provider)
        .build()
        .await
        .unwrap()
}

/// The D-Bus error name, which zbus puts first in the message.
fn error_name(e: zbus::Error) -> String {
    e.to_string().split(':').next().unwrap_or_default().to_string()
}

async fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn search_and_play_on_a_private_bus() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(bus) = PrivateBus::start(tmp.path(), &[]) else {
        eprintln!("dbus-daemon is not installed; skipping");
        return;
    };
    let ran = tmp.path().join("ran");
    let db = library(tmp.path(), &ran, 2);
    let _service = serve(&bus.address, db).await;
    let launcher = LauncherProxy::new(&connect(&bus.address).await).await.unwrap();

    let rows = launcher.search("test nes").await.unwrap();
    assert!(!rows.is_empty() && rows.len() <= 20, "{rows:#?}");
    let (id, title, subtitle, icon, actions, _) = rows[0].clone();
    assert!(id.starts_with("g:"), "{id}");
    assert!(title.starts_with("Test NES"), "{title}");
    assert!(!title.contains('('), "tags left out of the title: {title}");
    assert!(subtitle.contains("Nintendo"), "{subtitle}");
    assert_eq!((icon.as_str(), actions.as_str()), ("romforge", r#"["play"]"#));
    assert!(rows.windows(2).all(|w| w[0].5 >= w[1].5));
    assert!(launcher.search("x\u{1}y").await.unwrap().is_empty());
    assert!(launcher.search(&"a".repeat(300)).await.unwrap().is_empty());
    assert!(launcher.search("no such game").await.unwrap().is_empty());

    // Only a game in the library, and only `play`.
    for (bad, action) in [
        ("g:-:not a game", ""),
        ("/usr/bin/sh", "play"),
        ("$(id)", ""),
        ("", ""),
        (id.as_str(), "rm"),
        (id.as_str(), "PLAY"),
    ] {
        let e = launcher.activate(bad, action).await.unwrap_err();
        assert_eq!(
            error_name(e),
            "org.freedesktop.DBus.Error.InvalidArgs",
            "{bad:?} {action:?}"
        );
    }
    let long = format!("g:1:{}", "a".repeat(600));
    assert!(launcher.activate(&long, "").await.is_err());
    assert!(!ran.exists(), "nothing was started for a rejected call");

    // Enter: the stand-in gets the game's ROM, as Play gives it.
    launcher.activate(&id, "").await.unwrap();
    wait_for("the emulator", || ran.exists()).await;
    let args = std::fs::read_to_string(&ran).unwrap();
    assert!(args.trim_end().ends_with(".nes"), "{args}");
    // One game at a time.
    let e = launcher.activate(&id, "play").await.unwrap_err();
    assert_eq!(error_name(e), "org.freedesktop.DBus.Error.Failed");
    // Once it has ended, another may start.
    std::fs::remove_file(&ran).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match launcher.activate(&id, "play").await {
            Ok(()) => break,
            Err(_) if Instant::now() < deadline => tokio::time::sleep(Duration::from_millis(200)).await,
            Err(e) => panic!("the second start: {e}"),
        }
    }
    wait_for("the second start", || ran.exists()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_library_nothing_is_found_or_made() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(bus) = PrivateBus::start(tmp.path(), &[]) else {
        eprintln!("dbus-daemon is not installed; skipping");
        return;
    };
    let db = tmp.path().join("none/romforge.db");
    let _service = serve(&bus.address, db.clone()).await;
    let launcher = LauncherProxy::new(&connect(&bus.address).await).await.unwrap();
    assert!(launcher.search("mario").await.unwrap().is_empty());
    let e = launcher.activate("g:1:Mario", "").await.unwrap_err();
    assert_eq!(error_name(e), "org.freedesktop.DBus.Error.Failed");
    assert!(!db.exists() && !db.parent().unwrap().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bus_activation_and_idle_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let db = library(dir, &dir.join("ran"), 0);
    // The packaged service file, with the test binary in place of
    // /usr/bin/romforge-provider.
    let packaged = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../uq/dbus/org.romforge.Provider.service"),
    )
    .unwrap();
    assert!(packaged.contains(&format!("Name={BUS_NAME}\n")));
    assert!(packaged.contains("Exec=/usr/bin/romforge-provider\n"));
    let service = packaged.replace(
        "/usr/bin/romforge-provider",
        env!("CARGO_BIN_EXE_romforge-provider"),
    );
    // Written before the bus starts, so it is in its first scan.
    std::fs::create_dir_all(dir.join("services")).unwrap();
    std::fs::write(dir.join("services/org.romforge.Provider.service"), service).unwrap();
    let Some(bus) = PrivateBus::start(
        dir,
        &[
            ("ROMFORGE_DB", db.as_path()),
            ("ROMFORGE_PROVIDER_IDLE_SECS", Path::new("2")),
        ],
    ) else {
        eprintln!("dbus-daemon is not installed; skipping");
        return;
    };

    let conn = connect(&bus.address).await;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
    let activatable = dbus.list_activatable_names().await.unwrap();
    assert!(activatable.iter().any(|n| n.as_str() == BUS_NAME));

    // As uQ does: start it by name, then query only the running name.
    dbus.start_service_by_name(BUS_NAME.try_into().unwrap(), 0)
        .await
        .unwrap();
    let launcher = LauncherProxy::new(&conn).await.unwrap();
    let rows = launcher.search("test nes").await.unwrap();
    assert!(rows[0].1.starts_with("Test NES"), "{rows:#?}");

    // Two idle seconds later it is gone.
    let deadline = Instant::now() + Duration::from_secs(15);
    while dbus.name_has_owner(BUS_NAME.try_into().unwrap()).await.unwrap() {
        assert!(
            Instant::now() < deadline,
            "romforge-provider did not exit when idle"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(bus);
}
