//! The lists made by programs you have installed: ScummVM's detection
//! list (and its --detect) and MAME's own DAT.

use std::collections::BTreeSet;

use rusqlite::OptionalExtension;

use super::{ListsReport, set_stamp, stamp};
use crate::{Error, Library, launch, layout, scummvm, systems};

impl Library {
    /// ScummVM's list again when the installed ScummVM isn't the one that
    /// made it (or there is none); else only --detect for new folders.
    pub(super) fn scummvm_list_if_changed(&mut self, env: &launch::Env, report: &mut ListsReport) {
        let version = match launch::scummvm_version(env) {
            Ok(v) => v,
            Err(e) => return report.problems.push(format!("ScummVM: {e}")),
        };
        let have = self
            .extdb_dir()
            .is_some_and(|d| d.join(layout::SCUMMVM_LIST).is_file());
        // A stamp that can't be read: the list is made again.
        let made_by = match stamp(&self.conn, "list:scummvm") {
            Ok(v) => v,
            Err(e) => {
                report.problems.push(format!("ScummVM's list: {e}"));
                None
            }
        };
        if have && made_by.as_deref() == Some(version.as_str()) {
            return self.detect_new_roots(env, report);
        }
        match self.scummvm_read(env) {
            Ok(r) => {
                if let Err(e) =
                    set_stamp(&self.conn, "list:scummvm", &version).and_then(|()| self.remember_roots())
                {
                    report.problems.push(format!("ScummVM's list: {e}"));
                }
                report
                    .updated
                    .push(format!("ScummVM's list ({version}: {} games here)", r.games));
            }
            Err(e) => report.problems.push(format!("ScummVM's list: {e}")),
        }
    }

    /// ScummVM's --detect for the scanned folders it was never asked about
    /// (the engines its list leaves out).
    pub(super) fn detect_new_roots(&mut self, env: &launch::Env, report: &mut ListsReport) {
        let (asked, roots) = match stamp(&self.conn, "list:scummvm-roots")
            .and_then(|asked| Ok((asked, scummvm::top_roots(&self.conn)?)))
        {
            Ok(x) => x,
            Err(e) => return report.problems.push(format!("ScummVM --detect: {e}")),
        };
        let asked: BTreeSet<String> = asked
            .map(|v| v.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        let new: Vec<_> = roots
            .into_iter()
            .filter(|r| !asked.contains(&*r.to_string_lossy()))
            .collect();
        if new.is_empty() {
            return;
        }
        let mut found = Vec::new();
        for root in &new {
            match launch::scummvm_detect(env, root) {
                Ok(out) => found.extend(scummvm::parse_detect(&out, root)),
                Err(e) => report.problems.push(format!("ScummVM --detect: {e}")),
            }
        }
        let list = self.extdb_dir().map(|d| d.join(layout::SCUMMVM_LIST));
        let named =
            scummvm::remember(&self.conn, &found).and_then(|_| scummvm::refresh(&self.conn, list.as_deref()));
        match named {
            Ok(_) if !found.is_empty() => report
                .updated
                .push(format!("ScummVM looked in {} new folders", new.len())),
            Ok(_) => {}
            Err(e) => report.problems.push(format!("ScummVM --detect: {e}")),
        }
        if let Err(e) = self.remember_roots() {
            report.problems.push(format!("ScummVM --detect: {e}"));
        }
    }

    fn remember_roots(&self) -> Result<(), Error> {
        let roots: Vec<String> = scummvm::top_roots(&self.conn)?
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect();
        set_stamp(&self.conn, "list:scummvm-roots", &roots.join("\n"))
    }

    /// MAME's own DAT again when the installed MAME is newer than the one
    /// imported. Only for a library that has an Arcade DAT or a folder
    /// scanned for Arcade (the list is large and takes a minute).
    pub(super) fn mame_list_if_changed(
        &mut self,
        env: &launch::Env,
        report: &mut ListsReport,
    ) -> Result<(), Error> {
        let arcade = systems::by_key("Arcade").map_or(0, |s| s.id);
        let imported: Option<String> = self
            .conn
            .query_row(
                "SELECT source_path FROM dats WHERE system_id = ?1 AND source_path LIKE 'mame -listxml%'
                 ORDER BY dat_id DESC LIMIT 1",
                [arcade],
                |r| r.get(0),
            )
            .optional()?;
        let scoped: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM scan_roots WHERE system_id = ?1)",
            [arcade],
            |r| r.get(0),
        )?;
        if imported.is_none() && !scoped {
            return Ok(());
        }
        let version = match launch::mame_version(env) {
            Ok(v) => v,
            Err(e) => {
                report.problems.push(format!("MAME: {e}"));
                return Ok(());
            }
        };
        if imported.as_deref() == Some(format!("mame -listxml ({version})").as_str()) {
            return Ok(());
        }
        match self.mame_dat(env) {
            Ok(d) => report
                .updated
                .push(format!("MAME's list ({version}: {} machines)", d.games)),
            Err(e) => report.problems.push(format!("MAME's list: {e}")),
        }
        Ok(())
    }
}
