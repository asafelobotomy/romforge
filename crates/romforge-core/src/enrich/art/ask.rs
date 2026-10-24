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

/// Stream a video snap from `get` to the game's file; whether it was kept.
fn video(sh: &Shared, game: &str, get: &str, source: &str, url: &str, got: &mut Vec<Got>) -> bool {
    let path = art_path(sh.opts, sh.system, game, ArtKind::Video, "mp4");
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        got.push(Got::Error(format!("{}: {e}", dir.display())));
        return false;
    }
    match fetch::download_to(get, &path, MAX_VIDEO, |_, _| {}) {
        Ok(_) if is_mp4(&path) => {
            got.push(Got::Saved {
                kind: ArtKind::Video,
                path: path.to_string_lossy().into_owned(),
                source: source.to_owned(),
                url: url.to_owned(),
            });
            true
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&path);
            got.push(Got::Error(format!("{source}: {url} is not an MP4 video")));
            false
        }
        Err(e) => {
            got.push(Got::Error(format!("{source}: {e}")));
            false
        }
    }
}

/// Ask `provider` about the job's game and keep what it has of the kinds
/// still `wanted` (the best of each first), and its text. A source that
/// says "slow down" isn't asked again this run.
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
        .contains(&provider)
    {
        return;
    }
    match crate::artsources::lookup(provider, sh.keys, &sh.opts.sources, session, job) {
        Ok(found) => {
            if !found.fields.is_empty() {
                got.push(Got::Fields(provider, found.fields));
            }
            for ArtFound { kind, url, .. } in found.art {
                if !wanted.contains(&kind) {
                    continue;
                }
                let get = crate::artsources::download_url(provider.name(), &url, sh.keys, &sh.opts.sources);
                let kept = if kind == ArtKind::Video {
                    video(sh, job.title, &get, provider.name(), &url, got)
                } else {
                    match image(&get, sh.halted_hosts) {
                        Ok(Some(data)) => keep(sh, job.title, kind, &data, provider.name(), &url, got),
                        Ok(None) => false,
                        Err(e) => {
                            got.push(Got::Error(e.to_string()));
                            false
                        }
                    }
                };
                if kept {
                    wanted.retain(|k| *k != kind);
                }
            }
            got.push(Got::Asked(provider));
        }
        Err(e) => {
            if fetch::is_rate_limited(&e) {
                sh.halted.lock().unwrap_or_else(|e| e.into_inner()).push(provider);
            }
            got.push(Got::Error(format!("{}: {e}", provider.name())));
        }
    }
}
