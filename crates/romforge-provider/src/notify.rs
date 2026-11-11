//! Telling the user what went wrong: uQ shows only that the provider
//! failed, so a game that won't start (or stops at once) is also said in a
//! desktop notification (`org.freedesktop.Notifications`). Without a
//! notification server it is only logged.

use romforge_core::launch::Ended;

/// The app's icon, installed by its package.
pub const ICON: &str = "romforge";

/// What to say about a game that ended, if anything: only a failed start.
pub fn ended_message(e: &Ended) -> Option<(String, String)> {
    e.failed.then(|| {
        let how = match e.code {
            Some(code) => format!("ended with code {code}"),
            None => "was stopped by a signal".to_owned(),
        };
        (
            format!("{} didn't start", e.playing.game),
            format!(
                "{} {how} within seconds. Check its setup in romforge.",
                e.playing.emulator
            ),
        )
    })
}

/// Send a notification and log it; never fails.
pub async fn send(conn: &zbus::Connection, summary: &str, body: &str) {
    eprintln!("romforge-provider: {summary}: {body}");
    let hints: std::collections::HashMap<&str, zbus::Variant> = Default::default();
    let _ = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "romforge",
                0u32,
                ICON,
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use romforge_core::launch::Playing;

    fn ended(code: Option<i32>, failed: bool) -> Ended {
        Ended {
            playing: Playing {
                pid: 1,
                game: "Test NES".into(),
                system: "nes".into(),
                emulator: "retroarch:nestopia".into(),
                started: 0,
                skipped: Vec::new(),
            },
            seconds: 2,
            code,
            failed,
        }
    }

    #[test]
    fn only_a_failed_start_is_said() {
        assert_eq!(ended_message(&ended(Some(0), false)), None);
        let (summary, body) = ended_message(&ended(Some(1), true)).unwrap();
        assert_eq!(summary, "Test NES didn't start");
        assert!(body.starts_with("retroarch:nestopia ended with code 1"), "{body}");
        let (_, body) = ended_message(&ended(None, true)).unwrap();
        assert!(body.contains("stopped by a signal"), "{body}");
    }
}
