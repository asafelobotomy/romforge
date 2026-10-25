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
