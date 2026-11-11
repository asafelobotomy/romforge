//! `org.uq.Launcher1.Provider` at `/org/uq/Launcher1`, as uQ's launcher
//! bridge calls it:
//!
//! - `Search(s query) -> a(sssssd)`: up to 20 rows of (id, title, subtitle,
//!   icon, actions as a JSON list, score 0-1). The id is the library row's.
//! - `Activate(s id, s action)`: `play` (Enter) starts the game. The id must
//!   be a game in the library that Play may start; anything else is
//!   `InvalidArgs`.

use std::path::Path;
use std::sync::Arc;

use romforge_core::uq::launcher::Hit;
use tokio::sync::Notify;

use crate::games::{Games, PlayError};
use crate::notify::{self, ICON};

pub const BUS_NAME: &str = "org.romforge.Provider";
pub const PATH: &str = "/org/uq/Launcher1";
pub const INTERFACE: &str = "org.uq.Launcher1.Provider";
pub const ACTIONS_JSON: &str = r#"["play"]"#;
/// Longer actions are refused unread.
const MAX_ACTION_BYTES: usize = 32;

/// `(id, title, subtitle, icon, actions as a JSON list, score)`.
pub type Row = (String, String, String, String, String, f64);

pub struct Provider {
    pub games: Arc<Games>,
    /// Poked on every call (and when a game ends) so the idle timer starts
    /// over.
    pub touched: Arc<Notify>,
}

/// A launcher row: the game's box art as its icon when it is a file
/// here, else romforge's.
pub fn row(h: Hit) -> Row {
    let icon = h
        .picture
        .filter(|p| Path::new(p).is_absolute() && Path::new(p).is_file())
        .unwrap_or_else(|| ICON.to_owned());
    (h.id, h.title, h.subtitle, icon, ACTIONS_JSON.into(), h.score)
}

/// `""` is uQ's "the default action" (Enter).
pub fn check_action(action: &str) -> Result<(), String> {
    match action {
        "" | "play" => Ok(()),
        other => {
            let shown: String = other.chars().take(MAX_ACTION_BYTES).collect();
            Err(format!("unknown action {shown:?}"))
        }
    }
}

fn invalid(msg: impl Into<String>) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(msg.into())
}

#[zbus::interface(name = "org.uq.Launcher1.Provider")]
impl Provider {
    async fn search(&self, query: String) -> Vec<Row> {
        self.touched.notify_one();
        let games = self.games.clone();
        let found = tokio::task::spawn_blocking(move || games.search(&query)).await;
        match found {
            Ok(Ok(hits)) => hits.into_iter().map(row).collect(),
            Ok(Err(e)) => {
                eprintln!("romforge-provider: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }

    async fn activate(
        &self,
        id: String,
        action: String,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        self.touched.notify_one();
        check_action(&action).map_err(invalid)?;
        if !romforge_core::uq::launcher::id_shape(&id) {
            return Err(invalid("unknown game"));
        }
        let games = self.games.clone();
        let touched = self.touched.clone();
        let (bus, rt) = (conn.clone(), tokio::runtime::Handle::current());
        let started = tokio::task::spawn_blocking(move || {
            games.play(&id, move |ended| {
                if let Some((summary, body)) = notify::ended_message(ended) {
                    rt.spawn(async move { notify::send(&bus, &summary, &body).await });
                }
                // The idle minute starts when the game has ended.
                touched.notify_one();
            })
        })
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        match started {
            Ok(_) => Ok(()),
            Err(PlayError::Unknown) => Err(invalid("unknown game")),
            Err(PlayError::Failed(why)) => {
                notify::send(conn, "romforge couldn't start the game", &why).await;
                Err(zbus::fdo::Error::Failed(why))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions() {
        assert_eq!(check_action(""), Ok(()));
        assert_eq!(check_action("play"), Ok(()));
        assert!(check_action("Play").is_err());
        assert!(check_action("rm -rf /").is_err());
        assert!(check_action(&"x".repeat(5000)).unwrap_err().len() < 64);
    }

    #[test]
    fn rows_use_box_art_only_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("boxart.png");
        std::fs::write(&art, b"png").unwrap();
        let hit = |picture: Option<String>| Hit {
            id: "g:1:Test".into(),
            title: "Test".into(),
            subtitle: "NES".into(),
            picture,
            score: 0.9,
        };
        let r = row(hit(Some(art.display().to_string())));
        assert_eq!(r.3, art.display().to_string());
        assert_eq!(r.4, r#"["play"]"#);
        for gone in [
            Some(dir.path().join("none.png").display().to_string()),
            Some("relative.png".into()),
            None,
        ] {
            assert_eq!(row(hit(gone)).3, ICON);
        }
    }
}
