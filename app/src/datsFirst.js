// Before Scan folder: with no DAT imported a scan identifies nothing (every
// file stays unknown), so offer to download them first. Resolves once the
// scan may go ahead: always, whether the user downloads them or not.
// `run` is the jobs queue's run (the download is a job like any other).
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";

export async function datsFirst(run) {
  // Can't tell (the library is busy, say): scan as asked.
  const has = await invoke("has_dats").catch(() => true);
  if (has) return;
  const download = await ask(
    "No DATs are downloaded yet, so romforge can't identify any game in this folder: every file would stay unidentified. " +
      "Download the DATs first (a few minutes the first time), then scan?",
    { title: "Download DATs first?", kind: "info", okLabel: "Download DATs, then scan", cancelLabel: "Scan without them" },
  );
  if (download) await run("Downloading DATs… (a few minutes the first time)", "download_dats", { extras: false }, { key: "download_dats" });
}
