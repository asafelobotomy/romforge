//! How a found patch was paired with its game, in words.

use super::FoundPatch;

impl FoundPatch {
    /// [`FoundPatch::paired_by`] as a short phrase for people.
    pub fn paired_how(&self) -> &'static str {
        match self.paired_by.as_str() {
            "source" => "made from its ROM",
            "names" => "names its file",
            "beside" => "beside it, same name",
            "title" => "named after the game",
            "readme" => "its readme names the game",
            _ => "in its folder",
        }
    }
}
