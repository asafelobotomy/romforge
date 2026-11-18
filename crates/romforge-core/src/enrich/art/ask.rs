//! Asking one picture source for a game on a worker, and keeping what it
//! has: pictures as they come, a video snap streamed to its file.

use super::game::{Got, Shared, art_path, image};
use super::{image_ext, save};
use crate::enrich::{ArtFound, ArtKind};
use crate::fetch;

/// A video snap is at most this big (they are 2–30 MB).
const MAX_VIDEO: u64 = 64 << 20;

/// Keep `data` as the game's picture of `kind`; whether it was saved.
pub(super) fn keep(
    sh: &Shared,
    game: &str,
    kind: ArtKind,
    data: &[u8],
    source: &str,
    url: &str,
    got: &mut Vec<Got>,
) -> bool {
    let ext = image_ext(data).unwrap_or("png");
    let path = art_path(sh.opts, sh.system, game, kind, ext);
    match save(&path, data) {
        Ok(()) => {
            got.push(Got::Saved {
                kind,
                path: path.to_string_lossy().into_owned(),
                source: source.to_owned(),
                url: url.to_owned(),
            });
            true
        }
        Err(e) => {
            got.push(Got::Error(format!("{}: {e}", path.display())));
            false
        }
    }
}

/// Whether a file starts like an MP4 (an `ftyp` box first).
pub(super) fn is_mp4(path: &std::path::Path) -> bool {
    use std::io::Read as _;
    let mut head = [0u8; 12];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head[4..8] == b"ftyp"
}

/// A video snap's deadline, body included: the Arcade Database is asked
/// one request at a time, so a stalled one mustn't hold it for long.
const VIDEO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How one download for a wanted kind went.
enum Fetched {
    Kept,
    /// Not there, or not what it should be: an answer.
    No,
    /// Not reached (offline, rate-limited, cut short): asked again later.
    Failed,
}

/// Stream a video snap from `get` to the game's file (via a file of its
/// own, so a bad download never replaces a good one), unless its host
/// rate-limited this run.
fn video(sh: &Shared, game: &str, get: &str, source: &str, url: &str, got: &mut Vec<Got>) -> Fetched {
    let path = art_path(sh.opts, sh.system, game, ArtKind::Video, "mp4");
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        got.push(Got::Error(format!("{}: {e}", dir.display())));
        return Fetched::Failed;
    }
    let host = fetch::host_of(get).to_owned();
    if sh
        .halted_hosts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&host)
    {
        got.push(Got::Error(format!(
            "{source}: {host}: rate limiting, not asked again this run"
        )));
        return Fetched::Failed;
    }
    let new = path.with_extension("mp4.new");
    let req = fetch::Request {
        timeout: Some(VIDEO_TIMEOUT),
        ..fetch::Request::get(get)
    };
    match fetch::download_with(&req, &new, MAX_VIDEO, |_, _| {}) {
        Ok(_) if is_mp4(&new) => match std::fs::rename(&new, &path) {
            Ok(()) => {
                got.push(Got::Saved {
                    kind: ArtKind::Video,
                    path: path.to_string_lossy().into_owned(),
                    source: source.to_owned(),
                    url: url.to_owned(),
                });
                Fetched::Kept
            }
            Err(e) => {
                let _ = std::fs::remove_file(&new);
                got.push(Got::Error(format!("{}: {e}", path.display())));
                Fetched::Failed
            }
        },
        Ok(_) => {
            let _ = std::fs::remove_file(&new);
            got.push(Got::Error(format!("{source}: {url} is not an MP4 video")));
            Fetched::No
        }
        Err(e) => {
            if fetch::is_rate_limited(&e) {
                sh.halted_hosts
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(host);
            }
            got.push(Got::Error(format!("{source}: {e}")));
            Fetched::Failed
        }
    }
}

/// Ask `provider` about the job's game and keep what it has of the kinds
/// still `wanted` (the best of each first), and its text. A source that
/// says "slow down" isn't asked again this run.
/// Unreachable answers in a row (no connection, no answer in time, a
/// server error) after which a source isn't asked again this run.
pub(crate) const STRIKES: u32 = 3;

/// The keyed sources not asked again this run, by any worker: one that
/// said "slow down" or refused its key at once, one that is down after
/// [`STRIKES`] unreachable answers in a row (an answer resets its count).
#[derive(Debug, Default)]
pub(crate) struct Sources {
    halted: Vec<crate::artsources::Provider>,
    failing: std::collections::HashMap<crate::artsources::Provider, u32>,
}

impl Sources {
    pub(crate) fn halted(&self, p: crate::artsources::Provider) -> bool {
        self.halted.contains(&p)
    }

    /// `p` answered: its failures so far don't count.
    pub(crate) fn answered(&mut self, p: crate::artsources::Provider) {
        self.failing.remove(&p);
    }

    /// `p` failed with `e`; whether it is now left alone.
    pub(crate) fn failed(&mut self, p: crate::artsources::Provider, e: &crate::Error) -> bool {
        let down = fetch::is_unreachable(e) && {
            let n = self.failing.entry(p).or_default();
            *n += 1;
            *n >= STRIKES
        };
        if (fetch::stops_source(e) || down) && !self.halted(p) {
            self.halted.push(p);
        }
        self.halted(p)
    }
}

pub(super) fn ask(
    sh: &Shared,
    provider: crate::artsources::Provider,
    job: &crate::artsources::Job,
    wanted: &mut Vec<ArtKind>,
    got: &mut Vec<Got>,
    session: &mut crate::artsources::Session,
) {
    if wanted.is_empty() && !provider.has_text() {
        return;
    }
    if sh
        .halted
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .halted(provider)
    {
        return;
    }
    // Nothing to ask it with yet (no hashed ROM, no serial): not asked,
    // so not marked as asked either.
    if !crate::artsources::can_ask(provider, sh.keys, job) {
        return;
    }
    match crate::artsources::lookup(provider, sh.keys, &sh.opts.sources, session, job) {
        Ok(found) => {
            sh.halted
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .answered(provider);
            // Kinds a download of failed for.
            let mut failed: Vec<ArtKind> = Vec::new();
            if !found.fields.is_empty() {
                got.push(Got::Fields(provider, found.fields));
            }
            for ArtFound { kind, url, .. } in found.art {
                if !wanted.contains(&kind) {
                    continue;
                }
                let get = crate::artsources::download_url(provider.name(), &url, sh.keys, &sh.opts.sources);
                let fetched = if kind == ArtKind::Video {
                    video(sh, job.title, &get, provider.name(), &url, got)
                } else {
                    match image(&get, sh.halted_hosts) {
                        Ok(Some(data)) if keep(sh, job.title, kind, &data, provider.name(), &url, got) => {
                            Fetched::Kept
                        }
                        Ok(Some(_)) => Fetched::Failed,
                        Ok(None) => Fetched::No,
                        Err(e) => {
                            got.push(Got::Error(e.to_string()));
                            Fetched::Failed
                        }
                    }
                };
                match fetched {
                    Fetched::Kept => wanted.retain(|k| *k != kind),
                    Fetched::Failed => failed.push(kind),
                    Fetched::No => {}
                }
            }
            // A picture it has that wasn't reached (and no other of the
            // kind was kept): asked again next time.
            if !failed.iter().any(|k| wanted.contains(k)) {
                got.push(Got::Asked(provider));
            }
        }
        Err(e) => {
            let halted = sh
                .halted
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .failed(provider, &e);
            let note = if halted && fetch::is_unreachable(&e) {
                " (not asked again this run)"
            } else {
                ""
            };
            got.push(Got::Error(format!("{}: {e}{note}", provider.name())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artsources::Provider;

    /// A source down three times in a row is left alone; an answer in
    /// between starts the count again; a refused key stops it at once.
    #[test]
    fn a_source_that_keeps_failing_is_left_alone() {
        let down = || crate::Error::Unreachable("api: timed out".into());
        let mut s = Sources::default();
        let p = Provider::ScreenScraper;
        assert!(!s.failed(p, &down()));
        assert!(!s.failed(p, &down()));
        s.answered(p);
        assert!(!s.failed(p, &down()));
        assert!(
            !s.failed(p, &crate::Error::Http("api: HTTP 400".into())),
            "not a sign it's down"
        );
        assert!(!s.failed(p, &down()));
        assert!(s.failed(p, &down()), "third in a row");
        assert!(s.halted(p));
        let q = Provider::Igdb;
        assert!(s.failed(q, &crate::Error::Refused("api: HTTP 401".into())));
    }
}
