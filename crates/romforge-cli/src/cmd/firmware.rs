//! `firmware`.

use anyhow::Result;

use romforge_core::Library;
use romforge_core::firmware::{Match, State, TargetState};

use crate::args::Command;
use crate::subcommands::FirmwareCommand;

pub fn run(lib: &mut Library, command: Command) -> Result<()> {
    match command {
        Command::Firmware(FirmwareCommand::Download) => {
            let n = lib.firmware_download()?;
            println!("RetroBIOS's list: {n} emulators and cores");
        }
        Command::Firmware(FirmwareCommand::Check { system, all }) => {
            let r = lib.firmware_report()?;
            println!(
                "BIOS folder: {} (list of {})",
                r.bios_dir.display(),
                r.generated_at
            );
            for s in r
                .systems
                .iter()
                .filter(|s| system.as_deref().is_none_or(|k| s.key.eq_ignore_ascii_case(k)))
            {
                println!("{}", s.key);
                for e in &s.emulators {
                    let ready = match (e.ready, e.regions.is_empty()) {
                        (true, true) => "ready".to_owned(),
                        (true, false) => format!("ready for {}", e.regions.join(", ")),
                        (false, _) => "missing required files".to_owned(),
                    };
                    let installed = if e.installed { "" } else { ", not installed" };
                    let profiled = if e.profiled {
                        ""
                    } else {
                        ", not in RetroBIOS's list"
                    };
                    println!("  {} ({ready}{installed}{profiled})", e.label);
                    for f in &e.files {
                        if !all && !f.required && f.state == State::Missing {
                            continue;
                        }
                        let what = match f.state {
                            State::Accepted => "accepted",
                            State::Present => "present (no hash to check)",
                            State::WrongHash => "WRONG HASH",
                            State::Missing => "missing",
                        };
                        let need = match (f.required, f.hle, f.only_for.as_deref()) {
                            (true, _, Some(games)) => format!("only {games}"),
                            (true, false, None) => "required".to_owned(),
                            (true, true, None) => "required, runs without (HLE)".to_owned(),
                            _ => "optional".to_owned(),
                        };
                        let regions = if f.regions.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", f.regions.join("/"))
                        };
                        println!(
                            "    {:<28} {what:<26} {need}{regions}  {}",
                            f.name,
                            f.place.display()
                        );
                    }
                }
            }
        }
        Command::Firmware(FirmwareCommand::Import { paths, apply, also }) => {
            let plan = lib.firmware_import_plan(&paths)?;
            for f in &plan.files {
                let what = match f.matched {
                    Match::Accepted => "accepted",
                    Match::NameOnly => "name fits, hash unknown",
                    Match::Unknown => "unknown",
                };
                println!("{}  ({what})", f.shown);
                for t in &f.targets {
                    let state = match t.state {
                        TargetState::New => "goes to",
                        TargetState::AlreadyThere => "already in",
                        TargetState::Taken => "another file in (left alone)",
                        TargetState::Claimed => "another dropped file goes to",
                    };
                    println!("    {state} {}  [{}]", t.place.display(), t.used_by.join(", "));
                }
            }
            for (what, why) in &plan.skipped {
                println!("skipped {what}: {why}");
            }
            if apply {
                let done = lib.firmware_import(&paths, &also)?;
                println!("wrote {} file(s), linked {} more", done.written, done.linked);
            } else {
                println!(
                    "{} place(s) would be filled; run again with --apply",
                    plan.new_places
                );
            }
        }
        Command::Firmware(FirmwareCommand::InstallPs3 { apply }) => {
            if !apply {
                println!(
                    "would run RPCS3's firmware installer on Bios/rpcs3/PS3UPDAT.PUP (run again with --apply)"
                );
                return Ok(());
            }
            let flash = lib.install_ps3_firmware()?;
            println!("PS3 firmware installed into {}", flash.display());
        }
        Command::Firmware(FirmwareCommand::Undo { yes }) => {
            if !yes {
                anyhow::bail!("refusing to undo without --yes");
            }
            let n = lib.firmware_undo()?;
            println!("removed {n} file(s)");
        }
        _ => unreachable!(),
    }
    Ok(())
}
