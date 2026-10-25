//! How a program or a Flatpak is started.

use std::path::PathBuf;

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
            for g in grants {
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
