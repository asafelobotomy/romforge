//! gamescope's output size, which the ring's window takes there.

/// gamescope draws an external overlay unscaled, in its output's pixels,
/// from the output's top-left corner. Its X screen (and so the monitor GDK
/// reports) is the game's resolution (`-w`/`-h`), smaller than the output
/// when the game is upscaled (720p on a 1080p screen): a window that size
/// put the ring up and to the left of the middle. gamescope publishes the
/// output's size nowhere a client can read (its X screen, its Wayland
/// output and its screenshots are all the game's size), so it is worked
/// out: `ROMFORGE_OVERLAY_SIZE` (`WIDTHxHEIGHT`) when set; else
/// gamescope's own `-W`/`-H`; else, when it has `-w`/`-h` (and so may be
/// upscaling), the connected screen's preferred mode, which gamescope
/// takes on its own (DRM) and fills when fullscreen. `None`: the X
/// screen's size, which is the output's when there is no `-w`/`-h`.
pub(super) fn output() -> Option<(u32, u32)> {
    if let Some(size) = std::env::var("ROMFORGE_OVERLAY_SIZE")
        .ok()
        .and_then(|v| parse_size(&v))
    {
        return Some(size);
    }
    let args = args()?;
    let size = GamescopeSize::from_args(&args);
    if let Some(out) = size.output() {
        return Some(out);
    }
    if size.nested {
        return screen_mode();
    }
    None
}

/// The command line of the gamescope this runs in: the nearest ancestor
/// process named `gamescope`.
fn args() -> Option<Vec<String>> {
    let mut pid = std::process::id();
    for _ in 0..64 {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // `pid (comm) state ppid ...`; comm may hold spaces and brackets.
        let ppid: u32 = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
        if ppid <= 1 {
            return None;
        }
        pid = ppid;
        let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let args: Vec<String> = cmdline
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        let name = args.first().map(|a| a.rsplit('/').next().unwrap_or(a));
        if name == Some("gamescope") {
            return Some(args);
        }
    }
    None
}

/// What gamescope's command line says of its sizes.
#[derive(Debug, Default, PartialEq)]
struct GamescopeSize {
    width: Option<u32>,
    height: Option<u32>,
    /// `-w`/`-h` given: the game's resolution may be below the output's.
    nested: bool,
}

impl GamescopeSize {
    /// Read up to `--`, after which is the game's own command line.
    fn from_args(args: &[String]) -> Self {
        let mut size = Self::default();
        let mut it = args.iter().skip(1);
        while let Some(arg) = it.next() {
            if arg == "--" {
                break;
            }
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
                _ => match arg.as_str() {
                    a if a.len() > 2 && a.starts_with('-') && !a.starts_with("--") => {
                        (&a[..2], Some(a[2..].to_string()))
                    }
                    a => (a, None),
                },
            };
            let which = match flag {
                "-W" | "--output-width" => 'W',
                "-H" | "--output-height" => 'H',
                "-w" | "--nested-width" | "-h" | "--nested-height" => 'n',
                _ => continue,
            };
            let value = inline.or_else(|| it.next().cloned());
            let n = value.and_then(|v| v.parse::<u32>().ok()).filter(|n| *n > 0);
            match which {
                'W' => size.width = n,
                'H' => size.height = n,
                _ => size.nested |= n.is_some(),
            }
        }
        size
    }

    /// The output's size as gamescope works it out from `-W`/`-H`: the
    /// width is 16:9 of the height when only that is given.
    fn output(&self) -> Option<(u32, u32)> {
        let h = self.height?;
        Some((self.width.unwrap_or(h * 16 / 9), h))
    }
}

/// The preferred mode of the connected screen (DRM's sysfs: a connector
/// that is connected and enabled, its first mode); the first one found
/// where there are several.
fn screen_mode() -> Option<(u32, u32)> {
    let mut found: Vec<(bool, (u32, u32))> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
            if read("status")?.trim() != "connected" {
                return None;
            }
            let enabled = read("enabled").is_some_and(|s| s.trim() == "enabled");
            let mode = parse_size(read("modes")?.lines().next()?)?;
            Some((enabled, mode))
        })
        .collect();
    found.sort_by_key(|(enabled, _)| !enabled);
    found.first().map(|(_, mode)| *mode)
}

/// `1920x1080` as (1920, 1080); a mode's `1920x1080i` too.
fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let h = h.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let (w, h) = (w.trim().parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::{GamescopeSize, parse_size};

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(String::from).collect()
    }

    #[test]
    fn sizes_parse() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size(" 2560X1440 "), Some((2560, 1440)));
        assert_eq!(parse_size("1920x1080i"), Some((1920, 1080)));
        assert_eq!(parse_size("1920"), None);
        assert_eq!(parse_size("0x1080"), None);
        assert_eq!(parse_size("wide x tall"), None);
    }

    #[test]
    fn gamescope_args_give_the_output() {
        let s = |l| GamescopeSize::from_args(&words(l));
        let upscaled = s("gamescope --backend wayland -f -w 1280 -h 720 -W 1920 -H 1080 -- mpv -W 5");
        assert_eq!(upscaled.output(), Some((1920, 1080)));
        assert!(upscaled.nested);
        let joined = s("/usr/bin/gamescope -w1280 -h720 --output-width=2560 --output-height=1440");
        assert_eq!(joined.output(), Some((2560, 1440)));
        // Only -H: 16:9 of it, as gamescope does.
        assert_eq!(s("gamescope -H 1080 -- game").output(), Some((1920, 1080)));
        // -w/-h without -W/-H: the screen's mode decides.
        let nested = s("gamescope -w 1280 -h 720 -f -- game");
        assert_eq!((nested.output(), nested.nested), (None, true));
        // Nothing: the X screen is the output.
        assert_eq!(s("gamescope -e -- steam -gamepadui"), GamescopeSize::default());
        // The game's own flags after `--` are not gamescope's.
        assert_eq!(s("gamescope -- game -H 600").output(), None);
    }
}
