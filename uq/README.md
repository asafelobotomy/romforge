# romforge in uQ

What this folder adds to the [uQ](https://github.com/asafelobotomy/uq) desktop. romforge's
package installs it unconditionally; without uQ nothing reads it.

| File | Installed to | What it does |
| --- | --- | --- |
| `launcher/romforge.provider` | `/usr/share/uq/launcher/` | Typing a game's name in Spotlight finds it in your library; Enter starts it |
| `dbus/org.romforge.Provider.service` | `/usr/share/dbus-1/services/` | Lets the session bus start `/usr/bin/romforge-provider` when uQ's launcher opens |
| `integrations/romforge/rules.toml` | `/usr/share/uq/integrations/romforge/` | "Emulators full screen on the game workspace", off until you turn it on |

The package also installs `/usr/bin/romforge-provider` (from `crates/romforge-provider`).

## The launcher provider

`romforge-provider` is a small session-bus service on `org.romforge.Provider` that serves
`org.uq.Launcher1.Provider` at `/org/uq/Launcher1`. It is activatable: uQ asks the bus to start it
when Spotlight or the app menu opens (at most once every 30 s), and it exits after 60 s without a
call, but never while a game it started is running. The romforge app does not need to be open.

- **Search.** No prefix: every query of two characters or more. It opens the library the app uses
  (`$ROMFORGE_DB`, else the RomForge folder's, else `~/.local/share/romforge/romforge.db`), read
  as the app's list, and never makes one: without a library there are no rows. Only games Play can
  start are offered, by their clean name ("The Legend of Zelda - A Link to the Past"), with the
  platform and the version (regions, revision, translation, hack) as the subtitle and the box art
  as the icon when it has some. Up to 20 rows, best match first.
- **Play** (`play`, Enter) starts the game as the app's Play does: the emulator romforge picks for
  it (or the one set for that game), romforge's config for it written first, its play time
  recorded when it ends. One game at a time. If it won't start, or stops within seconds, a
  desktop notification says why (uQ itself only says that romforge failed). Hotkeys and the
  radial menu belong to the app, so they work only when the app started the game.
- **Input checks.** Queries over 256 bytes or with control characters get no rows. `Activate`
  takes only the id of a game in the library that Play may start (never a path, never anything
  run through a shell) and only the `play` action; anything else is `InvalidArgs`. ROM files are
  never touched.
- The provider gets its environment (`WAYLAND_DISPLAY` for the emulator) from the session bus's
  activation environment, which uQ's session sets.

## The game-mode rule

`rules.toml` offers one window rule, `romforge.emulators`: RetroArch, DuckStation, PPSSPP,
Dolphin, PCSX2, RPCS3 and Azahar open full screen on the workspace named `game`. uqd checks it and
renders it only after you turn it on in uQ Settings → Window rules → "Rules from apps"; turn it off
or undo it there. For a quieter game workspace, set its layout to master with no gaps and no
animations on uQ's Look and Window rules pages.

## How it was tested

- `uqctl integrations check uq/` is clean: 1 provider, 1 service file, 1 rule file.
- `cargo test -p romforge-core uq` covers the matching and ranking, the 20-row cap, which rows are
  offered, and the query and id checks; `cargo test -p romforge-provider` the action check, the
  icon choice and the failed-start notification.
- `crates/romforge-provider/tests/dbus.rs` runs on a private `dbus-daemon`: `Search` and
  `Activate` as uQ calls them against a library made from the repository's fixtures, the error
  names for bad ids and actions, a game started through a stand-in emulator, one game at a time,
  no library at all (nothing found, nothing made), and bus activation of the real binary from the
  packaged service file through to its idle exit.

By hand, with romforge installed:

```sh
busctl --user call org.romforge.Provider /org/uq/Launcher1 org.uq.Launcher1.Provider Search s zelda
busctl --user call org.romforge.Provider /org/uq/Launcher1 org.uq.Launcher1.Provider Activate ss 'g:…' play
```
