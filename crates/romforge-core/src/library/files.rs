//! Organize, cleanup, convert and the trash.

use std::path::Path;

use crate::{Error, Library, ScanOptions, cleanup, organize};

impl Library {
    /// Work out where identified files would go (a dry run; nothing moves).
    /// With a RomForge folder, Import does this job (milestone 13); only
    /// undoing earlier batches is left to Organize there.
    pub fn organize_plan(&self, opts: &organize::OrganizeOptions) -> Result<organize::Plan, Error> {
        if self.home.is_some() {
            return Err(Error::Organize(
                "with a RomForge folder, Import does this (every version kept and grouped); \
                 undo still works for earlier Organize batches"
                    .into(),
            ));
        }
        organize::plan(&self.conn, opts)
    }

    /// Carry out an organize plan and journal it for [`Library::organize_undo`].
    pub fn organize_apply(&mut self, plan: &organize::Plan, dest: &Path) -> Result<organize::Applied, Error> {
        organize::apply(&mut self.conn, plan, dest)
    }

    /// Move a batch's files back (the latest batch by default).
    pub fn organize_undo(&mut self, batch_id: Option<i64>) -> Result<organize::Undone, Error> {
        organize::undo(&mut self.conn, batch_id)
    }

    pub fn organize_batches(&self) -> Result<Vec<organize::Batch>, Error> {
        organize::batches(&self.conn)
    }

    /// What cleanup & rename would do (milestone 10); nothing is touched.
    /// `only`: limit to these library row ids.
    pub fn cleanup_plan(&self, only: Option<&[String]>) -> Result<cleanup::CleanupPlan, Error> {
        cleanup::plan(&self.conn, only, self.home.as_ref(), None)
    }

    /// The import plan for the games in `from` (every folder outside the
    /// RomForge folder when `None`): where each goes in `Games/`, and what
    /// stays. Nothing is touched (milestone 13).
    pub fn import_plan(&self, from: Option<&Path>) -> Result<cleanup::CleanupPlan, Error> {
        let from = from.map(std::fs::canonicalize).transpose()?;
        cleanup::plan(&self.conn, None, self.home.as_ref(), from.as_deref())
    }

    /// Carry out a cleanup plan, then refresh so the library shows the new
    /// files.
    pub fn cleanup_apply(
        &mut self,
        plan: &cleanup::CleanupPlan,
        progress: impl FnMut(usize, &str),
    ) -> Result<cleanup::CleanupReport, Error> {
        let r = cleanup::apply(&mut self.conn, plan, self.home.as_ref(), progress)?;
        self.refresh(ScanOptions::default(), |_, _| {})?;
        Ok(r)
    }

    /// Convert's plan: disc images into CHD beside themselves (nothing is
    /// written).
    pub fn convert_plan(&self, only: Option<&[String]>) -> Result<cleanup::CleanupPlan, Error> {
        cleanup::convert_plan(&self.conn, only, self.home.as_ref())
    }

    /// Carry out Convert's plan (journaled like a cleanup, so undo puts the
    /// originals back), then refresh.
    pub fn convert_apply(
        &mut self,
        only: Option<&[String]>,
        progress: impl FnMut(usize, &str),
    ) -> Result<cleanup::CleanupReport, Error> {
        let plan = self.convert_plan(only)?;
        self.cleanup_apply(&plan, progress)
    }

    /// Remove a cleanup batch's new files and put its originals back (the
    /// latest batch by default), then refresh.
    pub fn cleanup_undo(&mut self, batch_id: Option<i64>) -> Result<cleanup::CleanupUndone, Error> {
        let u = cleanup::undo(&mut self.conn, batch_id)?;
        self.refresh(ScanOptions::default(), |_, _| {})?;
        Ok(u)
    }

    pub fn cleanup_batches(&self) -> Result<Vec<cleanup::Batch>, Error> {
        cleanup::batches(&self.conn)
    }

    /// What is in the trash (all of it, or batches older than N days).
    pub fn trash_contents(&self, older_than_days: Option<u32>) -> Result<cleanup::TrashSummary, Error> {
        cleanup::trash_contents(&self.conn, older_than_days)
    }

    /// Delete those originals for good; their batches can't be undone after.
    pub fn empty_trash(&mut self, older_than_days: Option<u32>) -> Result<cleanup::TrashSummary, Error> {
        cleanup::empty_trash(&self.conn, older_than_days)
    }
}
