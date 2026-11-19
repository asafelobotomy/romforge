use super::*;

fn layout(root: &Path) -> Layout {
    Layout {
        data: root.join("data"),
        system_data: vec![root.join("usr/share")],
    }
}

fn fake_provider(root: &Path, body: &[u8]) -> PathBuf {
    let p = root.join("src").join(BIN_NAME);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn adds_three_files_and_removes_them_again() {
    let tmp = tempfile::tempdir().unwrap();
    let (l, src) = (layout(tmp.path()), fake_provider(tmp.path(), b"v1"));
    let st = status_in(&l, Some(&src));
    assert!(!st.added && !st.packaged && !st.stale);

    add_in(&l, &src).unwrap();
    let st = status_in(&l, Some(&src));
    assert!(st.added && !st.stale, "{st:?}");
    assert_eq!(std::fs::read(l.binary()).unwrap(), b"v1");
    let svc = std::fs::read_to_string(l.service_file()).unwrap();
    assert!(svc.starts_with(MARK), "{svc}");
    assert!(svc.contains(&format!("Exec={}\n", l.binary().display())), "{svc}");
    assert!(svc.contains("Name=org.romforge.Provider\n"));
    let prov = std::fs::read_to_string(l.provider_file()).unwrap();
    assert!(prov.contains("BusName=org.romforge.Provider\n") && prov.contains("Activatable=true\n"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(l.binary()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    assert!(remove_in(&l).unwrap());
    assert!(!l.binary().exists() && !l.provider_file().exists() && !l.service_file().exists());
    assert!(
        !l.binary().parent().unwrap().exists(),
        "its empty folder goes too"
    );
    assert!(!remove_in(&l).unwrap(), "nothing left to remove");
}

#[test]
fn a_new_provider_is_copied_only_when_added() {
    let tmp = tempfile::tempdir().unwrap();
    let (l, src) = (layout(tmp.path()), fake_provider(tmp.path(), b"v1"));
    assert!(!refresh_in(&l, &src).unwrap(), "never adds on its own");
    assert!(!l.binary().exists());
    add_in(&l, &src).unwrap();
    assert!(!refresh_in(&l, &src).unwrap(), "same bytes: nothing to do");
    let src2 = fake_provider(tmp.path(), b"version two");
    assert!(status_in(&l, Some(&src2)).stale);
    assert!(refresh_in(&l, &src2).unwrap());
    assert_eq!(std::fs::read(l.binary()).unwrap(), b"version two");
}

#[test]
fn leaves_a_package_and_a_users_own_files_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let (l, src) = (layout(tmp.path()), fake_provider(tmp.path(), b"v1"));

    // A user's own file with that name is not romforge's to overwrite or remove.
    std::fs::create_dir_all(l.provider_file().parent().unwrap()).unwrap();
    std::fs::write(
        l.provider_file(),
        "[Launcher Provider]\nBusName=org.romforge.Provider\n",
    )
    .unwrap();
    assert!(add_in(&l, &src).unwrap_err().to_string().contains("yours"));
    assert!(!l.binary().exists(), "nothing written before the refusal");
    assert!(!remove_in(&l).unwrap());
    assert!(l.provider_file().is_file());
    std::fs::remove_file(l.provider_file()).unwrap();

    // A package's files mean there is nothing to add.
    let pkg = tmp.path().join("usr/share/uq/launcher/romforge.provider");
    std::fs::create_dir_all(pkg.parent().unwrap()).unwrap();
    std::fs::write(&pkg, "x").unwrap();
    assert!(status_in(&l, Some(&src)).packaged);
    assert!(add_in(&l, &src).unwrap_err().to_string().contains("package"));
}

#[test]
fn refuses_a_path_the_service_file_cannot_hold() {
    let tmp = tempfile::tempdir().unwrap();
    let src = fake_provider(tmp.path(), b"v1");
    let l = Layout {
        data: tmp.path().join("my data"),
        system_data: vec![],
    };
    assert!(add_in(&l, &src).unwrap_err().to_string().contains("space"));
    assert!(!l.data.exists(), "nothing written");
    assert!(add_in(&layout(tmp.path()), &tmp.path().join("missing")).is_err());
}
