//! `firmware`.

use anyhow::Result;

use romforge_core::Library;
use romforge_core::firmware::State;

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
                        let need = match (f.required, f.hle) {
                            (true, false) => "required",
                            (true, true) => "required, runs without (HLE)",
                            _ => "optional",
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
        _ => unreachable!(),
    }
    Ok(())
}
