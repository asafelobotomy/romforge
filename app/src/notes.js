// The status line's words after the toolbar's jobs.

export function refreshNote(r) {
  return (
    (r.lists?.length ? `Updated ${r.lists.join("; ")}. ` : "") +
    `Rescanned ${r.folders} folders, ${r.entries} entries` +
    (r.in_games ? `; ${r.in_games} files of ScummVM games named by ScummVM's list, not hashed` : "") +
    (r.skipped ? `; skipped for their extension: ${r.skipped}` : "") +
    (r.problems.length ? `; ${r.problems.length} problems: ${r.problems.join("; ")}` : "")
  );
}

export function scanNote(r) {
  const report = r.report;
  const problems = [
    ...report.errors.map(([path, member, e]) => `${member ? `${path}#${member}` : path}: ${e}`),
    ...report.sheet_problems.map(([path, e]) => `${path}: ${e}`),
  ];
  const kinds = report.skipped_kinds.slice(0, 6).map((k) => (k.ext ? `.${k.ext} ${k.count}` : `no extension ${k.count}`)).join(", ");
  // `became_source`: null (a Source was set already), {Ok: null}, or {Err: why}.
  const bs = r.became_source;
  const source = bs && "Ok" in bs
    ? "; it is now your Source folder (Settings), which Refresh rescans"
    : bs?.Err
      ? `; it couldn't become your Source folder: ${bs.Err}`
      : "";
  return (
    `Scanned ${report.hashed} entries in ${report.root}${source}` +
    (report.in_games ? `; ${report.in_games} files of ScummVM games named by ScummVM's list, not hashed` : "") +
    (kinds ? `; skipped for their extension: ${kinds}${report.skipped_kinds.length > 6 ? ", …" : ""}` : "") +
    (problems.length ? `. ${problems.length} problem${problems.length === 1 ? "" : "s"}: ${problems.slice(0, 3).join("; ")}${problems.length > 3 ? "; …" : ""}` : "")
  );
}

export function cleanupNote(r) {
  return r.batch_id
    ? `Rename & Cleanup batch #${r.batch_id}: ${r.done} games, ${r.trashed} originals to the trash` +
        (r.failed.length ? `; ${r.failed.length} failed: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "") +
        " (Undo in its dry run)"
    : "Rename & Cleanup: nothing to do" + (r.failed.length ? `: ${r.failed.map(([t, e]) => `${t}: ${e}`).join("; ")}` : "");
}

export function enrichNote(r) {
  let note = `${r.games} games: ${r.games_with_metadata} with metadata; ${r.art_saved} images saved, ${r.art_missing} not found, ${r.art_cached} already cached`;
  if (r.wikidata) note += `. Wikidata: ${r.wikidata.matched} matched, ${r.wikidata.ambiguous} to settle in Search, ${r.wikidata.unmatched} not found`;
  if (r.errors?.length) note += `. ${r.errors.length} problem${r.errors.length === 1 ? "" : "s"}: ${r.errors.slice(0, 3).join("; ")}${r.errors.length > 3 ? "; …" : ""}`;
  return note;
}
