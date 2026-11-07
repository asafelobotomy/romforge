//! Settings as a new library has them.

use super::*;

impl Default for Settings {
    fn default() -> Self {
        Self {
            source_dir: None,
            library_dir: None,
            keep_originals: false,
            in_place: false,
            staging_dir: None,
            bios_leave_in_place: true,
            keys: Keys::default(),
            show_empty_media: false,
            region_flags: true,
            show_missing_versions: false,
            regions: Vec::new(),
            language: String::new(),
            video_snaps: false,
            platform_icons: String::new(),
            threads: 0,
            list_age_days: default_list_age(),
            save_copies: crate::saves::backup::default_keep(),
            while_playing: default_while_playing(),
            start_fullscreen: false,
            attract_minutes: 0,
            screensaver_minutes: 0,
            music_dir: None,
            music_volume: default_volume(),
            music_shuffle: true,
            fs_details: default_fs_details(),
            fs_bookshelf: Bookshelf::default(),
        }
    }
}
