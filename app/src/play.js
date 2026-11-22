import { displayTitle } from "./labels.js";

// Starting a game, the same from the desktop and the fullscreen mode:
// never queued (a game starts at once, whatever job is running), then the
// splash with the art there is (`art`: { box3d, boxart, platform }).
export async function startGame(row, art, { call, nowPlaying, setNote }) {
  if (!row) return;
  setNote(`Starting ${displayTitle(row)}…`);
  const r = await call("launch", { rowId: row.id });
  setNote(r?.skipped?.length ? `Started with ${r.emulator}; passed over ${r.skipped.join("; ")}.` : "");
  if (!r) return;
  nowPlaying.started(r, { box3d: art?.box3d ?? null, boxart: art?.boxart ?? null, platform: art?.platform ?? row.system_name ?? r.system });
}

// Play on a row (the core decides, `play_choice`): a game with several
// playable versions offers them, `pick(versions)`, unless one was kept
// with "Always run", which starts at once; so does a version picked on its
// own, or a game with one. Resolves to the row to start, or null.
export async function playChoice(row, call, rows, pick) {
  if (!row) return null;
  const c = await call("play_choice", { rowId: row.id });
  if (!c) return null;
  if (c.run) return rows.find((r) => r.id === c.run) ?? row;
  pick(c.versions);
  return null;
}

