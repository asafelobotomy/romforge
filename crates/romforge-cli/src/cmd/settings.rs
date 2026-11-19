//! `settings`.

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args;

use romforge_core::{Library, settings};

use crate::args::Command;

/// The settings the app keeps under Display, Playing and Fullscreen.
#[derive(Args, Default)]
pub struct MoreSettings {
    /// Language descriptions are fetched in: en, fr, de, es, it, pt, nl, sv,
    /// pl, ru or ja ("" for the system's)
    #[arg(long)]
    language: Option<String>,
    /// List every DAT version of a game, even ones you don't have
    #[arg(long)]
    show_missing_versions: Option<bool>,
    /// Details shows a box for every kind of art, even empty ones
    #[arg(long)]
    show_empty_media: Option<bool>,
    /// Enrich downloads video snaps (2 to 30 MB each)
    #[arg(long)]
    video_snaps: Option<bool>,
    /// Version rows show region and language as flag emojis (false: text)
    #[arg(long)]
    region_flags: Option<bool>,
    /// The sidebar's platform icons: flatui, systematic, logo or none
    #[arg(long)]
    platform_icons: Option<String>,
    /// The window while a game plays: tray, minimise or stay
    #[arg(long)]
    while_playing: Option<String>,
    /// Fullscreen mode: attract mode after this many idle minutes (0, 3, 5
    /// or 10; 0 never)
    #[arg(long)]
    attract_minutes: Option<u32>,
    /// Fullscreen mode: screensaver this many minutes after attract mode
    /// starts (0, 10 or 30; 0 never)
    #[arg(long)]
    screensaver_minutes: Option<u32>,
    /// Fullscreen mode: a folder of your own music ("" for none)
    #[arg(long)]
    music_dir: Option<String>,
    /// Music volume, 0 to 100
    #[arg(long)]
    music_volume: Option<u32>,
    /// Music plays in random order
    #[arg(long)]
    music_shuffle: Option<bool>,
    /// Fullscreen mode: the details shown of the game in focus, comma
    /// separated (logo, platform, released, genre, ...; "" for none)
    #[arg(long, value_delimiter = ',')]
    fs_details: Option<Vec<String>>,
    /// Bookshelf layout: horizontal or vertical
    #[arg(long)]
    shelf_direction: Option<String>,
    /// Bookshelf layout, All games: platform or mixed
    #[arg(long)]
    shelf_group: Option<String>,
    /// Bookshelf layout: a shelf of favourites at the top
    #[arg(long)]
    shelf_favourites: Option<bool>,
    /// Bookshelf layout: a shelf of games played lately
    #[arg(long)]
    shelf_recent: Option<bool>,
    /// Bookshelf layout: title, released, newest or played
    #[arg(long)]
    shelf_sort: Option<String>,
    /// Bookshelf layout, where the details go: top, left or right
    #[arg(long)]
    shelf_details: Option<String>,
}

/// `value` as one of `choices` (their keys), or the list in the error.
fn one_of(what: &str, choices: &[(&str, &str)], value: &str) -> Result<String> {
    if choices.iter().any(|(k, _)| *k == value) {
        return Ok(value.to_owned());
    }
    let keys: Vec<_> = choices.iter().map(|(k, _)| *k).collect();
    bail!("no {what} {value:?}: {}", keys.join(", "))
}

impl MoreSettings {
    /// Put what was given into `s`; whether anything was. The settings'
    /// own checks (`save_settings`) cover the rest.
    fn apply(self, s: &mut settings::Settings) -> Result<bool> {
        let mut changed = false;
        macro_rules! set {
            ($given:expr, $slot:expr) => {
                if let Some(v) = $given {
                    $slot = v;
                    changed = true;
                }
            };
        }
        set!(self.language, s.language);
        set!(self.show_missing_versions, s.show_missing_versions);
        set!(self.show_empty_media, s.show_empty_media);
        set!(self.video_snaps, s.video_snaps);
        set!(self.region_flags, s.region_flags);
        set!(self.platform_icons, s.platform_icons);
        set!(self.while_playing, s.while_playing);
        set!(self.attract_minutes, s.attract_minutes);
        set!(self.screensaver_minutes, s.screensaver_minutes);
        set!(self.music_volume, s.music_volume);
        set!(self.music_shuffle, s.music_shuffle);
        set!(self.shelf_favourites, s.fs_bookshelf.favourites);
        set!(self.shelf_recent, s.fs_bookshelf.recent);
        if let Some(d) = self.music_dir {
            s.music_dir = (!d.is_empty()).then(|| PathBuf::from(d));
            changed = true;
        }
        if let Some(details) = self.fs_details {
            let details: Vec<String> = details.into_iter().filter(|d| !d.is_empty()).collect();
            if let Some(bad) = details
                .iter()
                .find(|d| !settings::FS_DETAILS.iter().any(|(k, _)| k == d))
            {
                let keys: Vec<_> = settings::FS_DETAILS.iter().map(|(k, _)| *k).collect();
                bail!("no detail {bad:?}: {}", keys.join(", "));
            }
            s.fs_details = details;
            changed = true;
        }
        if let Some(v) = self.shelf_direction {
            s.fs_bookshelf.direction = one_of("shelf direction", settings::SHELF_DIRECTIONS, &v)?;
            changed = true;
        }
        if let Some(v) = self.shelf_group {
            s.fs_bookshelf.group = one_of("shelf grouping", settings::SHELF_GROUPS, &v)?;
            changed = true;
        }
        if let Some(v) = self.shelf_sort {
            s.fs_bookshelf.sort = one_of("shelf order", settings::SHELF_SORTS, &v)?;
            changed = true;
        }
        if let Some(v) = self.shelf_details {
            s.fs_bookshelf.details = one_of("details place", settings::SHELF_DETAILS, &v)?;
            changed = true;
        }
        Ok(changed)
    }
}

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Settings {
            more,
            source,
            library,
            keep_originals,
            in_place,
            bios_leave_in_place,
            staging,
            regions,
            threads,
            list_age_days,
            save_copies,
            start_fullscreen,
            scan_downloads,
            steamgriddb_key,
            igdb_client_id,
            igdb_client_secret,
            screenscraper_dev_id,
            screenscraper_dev_password,
            screenscraper_user,
            screenscraper_password,
        } => {
            let mut s = lib.settings()?;
            let mut changed = (*more).apply(&mut s)?
                || source.is_some()
                || library.is_some()
                || keep_originals.is_some()
                || in_place.is_some()
                || bios_leave_in_place.is_some()
                || staging.is_some()
                || regions.is_some()
                || threads.is_some()
                || list_age_days.is_some()
                || save_copies.is_some()
                || start_fullscreen.is_some()
                || scan_downloads.is_some();
            for (given, slot) in [
                (steamgriddb_key, &mut s.keys.steamgriddb_key),
                (igdb_client_id, &mut s.keys.igdb_client_id),
                (igdb_client_secret, &mut s.keys.igdb_client_secret),
                (screenscraper_dev_id, &mut s.keys.screenscraper_dev_id),
                (screenscraper_dev_password, &mut s.keys.screenscraper_dev_password),
                (screenscraper_user, &mut s.keys.screenscraper_user),
                (screenscraper_password, &mut s.keys.screenscraper_password),
            ] {
                if let Some(v) = given {
                    *slot = Some(v);
                    changed = true;
                }
            }
            let set = |p: String| (!p.is_empty()).then(|| PathBuf::from(p));
            if let Some(p) = source {
                s.source_dir = set(p);
            }
            if let Some(p) = library {
                s.library_dir = set(p);
            }
            if let Some(b) = bios_leave_in_place {
                s.bios_leave_in_place = b;
            }
            if let Some(k) = keep_originals {
                s.keep_originals = k;
            }
            if let Some(k) = in_place {
                s.in_place = k;
            }
            if let Some(p) = staging {
                s.staging_dir = set(p);
            }
            if let Some(r) = regions {
                s.regions = settings::parse_regions(&r);
            }
            if let Some(n) = threads {
                s.threads = n;
            }
            if let Some(d) = list_age_days {
                s.list_age_days = d;
            }
            if let Some(n) = save_copies {
                s.save_copies = n;
            }
            if let Some(f) = start_fullscreen {
                s.start_fullscreen = f;
            }
            if let Some(f) = scan_downloads {
                s.scan_downloads = f;
            }
            if changed {
                s = lib.save_settings(&s)?;
            }
            let shown = |p: &Option<PathBuf>| p.as_ref().map_or("-".to_owned(), |p| p.display().to_string());
            println!("source          {}", shown(&s.source_dir));
            println!("library         {}", shown(&s.library_dir));
            println!("keep originals  {}", if s.keep_originals { "yes" } else { "no" });
            println!(
                "BIOS left where they are  {}",
                if s.bios_leave_in_place {
                    "yes"
                } else {
                    "no (moved to the trash)"
                }
            );
            println!(
                "start fullscr.  {}",
                if s.start_fullscreen { "yes" } else { "no" }
            );
            println!(
                "check downloads {}",
                if s.scan_downloads { "yes (oysterAV)" } else { "no" }
            );
            println!(
                "in place        {}",
                if s.in_place {
                    "yes (replace originals)"
                } else {
                    "no"
                }
            );
            println!("staging         {}", s.staging().display());
            println!("regions         {}", s.region_order().join(", "));
            println!(
                "list age        {}",
                match s.list_age_days {
                    0 => "never online on Refresh".to_owned(),
                    d => format!("{d} days"),
                }
            );
            println!("save copies     {}", s.save_copies);
            let yes = |b: bool| if b { "yes" } else { "no" };
            println!(
                "language        {}",
                if s.language.is_empty() {
                    format!("the system's ({})", s.language())
                } else {
                    s.language.clone()
                }
            );
            println!("missing versions listed  {}", yes(s.show_missing_versions));
            println!("empty art boxes shown    {}", yes(s.show_empty_media));
            println!("video snaps     {}", yes(s.video_snaps));
            println!(
                "region flags    {}",
                if s.region_flags { "flags" } else { "text" }
            );
            println!(
                "platform icons  {}",
                if s.platform_icons.is_empty() {
                    "flatui"
                } else {
                    &s.platform_icons
                }
            );
            println!("while playing   {}", s.while_playing);
            let minutes = |n: u32| {
                if n == 0 {
                    "never".to_owned()
                } else {
                    format!("{n} min")
                }
            };
            println!("attract mode    {}", minutes(s.attract_minutes));
            println!("screensaver     {}", minutes(s.screensaver_minutes));
            match &s.music_dir {
                Some(d) => println!(
                    "music           {} ({} tracks), volume {}, {}",
                    d.display(),
                    romforge_core::music::tracks(d).len(),
                    s.music_volume,
                    if s.music_shuffle { "shuffled" } else { "in order" }
                ),
                None => println!("music           none"),
            }
            println!("fs details      {}", s.fs_details.join(", "));
            let b = &s.fs_bookshelf;
            println!(
                "bookshelf       {}, {}, {} order, details {}; favourites shelf {}, recent shelf {}",
                b.direction,
                b.group,
                b.sort,
                b.details,
                yes(b.favourites),
                yes(b.recent)
            );
            println!(
                "threads         {}",
                match s.threads {
                    0 => format!("automatic ({})", romforge_core::parallel::threads(0)),
                    n => n.to_string(),
                }
            );
            let key = |v: &Option<String>| v.as_deref().map_or("-".to_owned(), settings::mask);
            println!("steamgriddb     {}", key(&s.keys.steamgriddb_key));
            println!(
                "igdb            {} / {}",
                key(&s.keys.igdb_client_id),
                key(&s.keys.igdb_client_secret)
            );
            println!(
                "screenscraper   dev {} / {}, user {}",
                key(&s.keys.screenscraper_dev_id),
                key(&s.keys.screenscraper_dev_password),
                key(&s.keys.screenscraper_user)
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}
