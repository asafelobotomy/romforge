//! How a program or a Flatpak is started.

use std::path::{Path, PathBuf};

use super::Runner;

/// The command line for `runner`. A Flatpak sees only what it was given
/// (often just home), so it gets `grants` (the game's folder, and the
/// RomForge folder for romforge's config) for this run: a game on another
/// drive opens too, and nothing is changed for good.
///
/// `env` is set for the emulator: in the process for a program; for a
/// Flatpak by starting its own command through the runtime's `env`
/// (`--command=env <id> K=V <command>`), as Flatpak sets its XDG folders
/// over any `--env=` (seen with Flatpak 1.18.2 on 2026-10-25), else as
/// `--env=` when its command isn't known. The third part is what the
/// process gets ([`Session::start`], [`spawn`]).
pub(super) fn command(
    runner: &Runner,
    args: Vec<String>,
    grants: &[PathBuf],
    env: &[(String, String)],
) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
    match runner {
        Runner::Program(p) => (p.clone(), args, env.to_vec()),
        Runner::Flatpak { id, command } => {
            let mut a = vec!["run".to_owned()];
            for g in grants.iter().filter_map(|g| grantable(g)) {
                a.push(format!("--filesystem={}", g.display()));
            }
            match command.as_ref().filter(|_| !env.is_empty()) {
                Some(c) => {
                    a.extend(["--command=env".to_owned(), id.clone()]);
                    a.extend(env.iter().map(|(k, v)| format!("{k}={v}")));
                    a.push(c.clone());
                }
                None => {
                    a.extend(env.iter().map(|(k, v)| format!("--env={k}={v}")));
                    a.push(id.clone());
                }
            }
            a.extend(args);
            (PathBuf::from("flatpak"), a, Vec::new())
        }
    }
}

/// What `--filesystem=` can grant for `p`: Flatpak reads `:ro`, `:rw` and
/// `:create` after a colon, so a folder whose path has one is granted
/// through its nearest parent without; nothing when that is `/`.
fn grantable(p: &Path) -> Option<&Path> {
    p.ancestors()
        .find(|a| !a.to_string_lossy().contains(':'))
        .filter(|a| a.is_absolute() && a.parent().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colon_in_a_path_is_never_read_as_a_mode() {
        assert_eq!(grantable(Path::new("/games/nes")), Some(Path::new("/games/nes")));
        assert_eq!(
            grantable(Path::new("/games/Zelda: LA/rom")),
            Some(Path::new("/games"))
        );
        assert_eq!(grantable(Path::new("/a:b/c")), None, "not all of /");
        let runner = Runner::Flatpak {
            id: "org.x.Emu".into(),
            command: None,
        };
        let (_, args, _) = command(&runner, vec![], &[PathBuf::from("/g/x:ro")], &[]);
        assert_eq!(args, ["run", "--filesystem=/g", "org.x.Emu"]);
    }
}
