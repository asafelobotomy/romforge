//! "Add to uQ search" for the AppImage, end to end on a private session
//! bus: romforge's own files in a user's data folder, no package. The bus
//! reads its service folders the way a session bus does
//! (`<standard_session_servicedirs/>`, so `$XDG_DATA_HOME/dbus-1/services`),
//! lists the provider as startable, starts it from the stable copy, and
//! stops listing it once the files are removed. Skipped when `dbus-daemon`
//! is not installed.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use romforge_core::uq::user_provider::{self, BUS_NAME, Layout};

struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start(dir: &Path, data: &Path) -> Option<Self> {
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <standard_session_servicedirs />
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                dir.join("bus").display()
            ),
        )
        .unwrap();
        let mut child = match Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .env("XDG_DATA_HOME", data)
            // Nothing else a bus might read.
            .env("XDG_DATA_DIRS", dir.join("no-such-folder"))
            .env("ROMFORGE_DB", dir.join("none/romforge.db"))
            .env("ROMFORGE_PROVIDER_IDLE_SECS", "2")
            .stdout(Stdio::piped())
            .spawn()
        {
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

    async fn connect(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn startable(conn: &zbus::Connection) -> bool {
    zbus::fdo::DBusProxy::new(conn)
        .await
        .unwrap()
        .list_activatable_names()
        .await
        .unwrap()
        .iter()
        .any(|n| n.as_str() == BUS_NAME)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_users_own_files_make_the_provider_startable_and_removing_them_undoes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let layout = Layout {
        data: dir.join("data"),
        system_data: vec![dir.join("no-such-folder")],
    };
    let source = Path::new(env!("CARGO_BIN_EXE_romforge-provider"));

    // Before: nothing offers the provider.
    {
        let Some(bus) = PrivateBus::start(dir, &layout.data) else {
            eprintln!("dbus-daemon is not installed; skipping");
            return;
        };
        assert!(!startable(&bus.connect().await).await);
    }

    // The user said yes: the three files.
    user_provider::add_in(&layout, source).unwrap();
    let st = user_provider::status_in(&layout, Some(source));
    assert!(st.added && !st.stale && !st.packaged, "{st:?}");

    {
        let bus = PrivateBus::start(dir, &layout.data).unwrap();
        let conn = bus.connect().await;
        assert!(startable(&conn).await, "the user's service folder is listed");

        // As uQ does: start it by name, then it answers on its name.
        let dbus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
        dbus.start_service_by_name(BUS_NAME.try_into().unwrap(), 0)
            .await
            .unwrap();
        let owner = dbus.get_name_owner(BUS_NAME.try_into().unwrap()).await.unwrap();
        let pid = dbus.get_connection_unix_process_id(owner.into()).await.unwrap();
        // It is the stable copy that runs, not the file it was copied from.
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap();
        assert_eq!(exe, layout.binary(), "started from the copy in the data folder");
    }

    // The user said remove: all three go, and the bus no longer offers it.
    assert!(user_provider::remove_in(&layout).unwrap());
    {
        let bus = PrivateBus::start(dir, &layout.data).unwrap();
        assert!(!startable(&bus.connect().await).await);
    }
    assert!(!user_provider::status_in(&layout, Some(source)).added);
}
