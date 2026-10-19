use super::*;

pub fn user_emulators(conn: &Connection) -> Result<Vec<UserEmulator>, Error> {
    let mut stmt = conn.prepare(
        "SELECT e.name, s.key, e.program, e.args, e.archives FROM emulators e
         JOIN systems s ON s.system_id = e.system_id ORDER BY e.emulator_id",
    )?;
    let rows = stmt.query_map([], |r| {
        let args: String = r.get(3)?;
        Ok(UserEmulator {
            name: r.get(0)?,
            system: r.get(1)?,
            program: r.get(2)?,
            args: serde_json::from_str(&args).unwrap_or_default(),
            archives: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_emulator(conn: &Connection, e: &UserEmulator) -> Result<(), Error> {
    let system = systems::by_key(&e.system).ok_or_else(|| Error::UnknownSystem(e.system.clone()))?;
    if e.args.iter().all(|a| !a.contains("{rom}")) {
        return Err(Error::Launch("the arguments need a {rom} placeholder".into()));
    }
    conn.execute(
        "INSERT INTO emulators (name, system_id, program, args, archives) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name, system_id) DO UPDATE SET program = excluded.program, args = excluded.args,
           archives = excluded.archives",
        params![
            e.name,
            system.id,
            e.program,
            serde_json::to_string(&e.args).unwrap_or_default(),
            e.archives
        ],
    )?;
    Ok(())
}

pub fn remove_emulator(conn: &Connection, name: &str) -> Result<usize, Error> {
    Ok(conn.execute("DELETE FROM emulators WHERE name = ?1", [name])?)
}

/// Every emulator romforge could use, and whether it is installed.
pub fn status(conn: &Connection, env: &Env) -> Result<Vec<EmulatorStatus>, Error> {
    let mut out = Vec::new();
    for u in user_emulators(conn)? {
        out.push(EmulatorStatus {
            found: env.which(&u.program).map(|p| p.display().to_string()),
            name: u.name,
            systems: vec![u.system],
        });
    }
    let ra = env.retroarch();
    let mut cores: Vec<&str> = systems::SYSTEMS
        .iter()
        .flat_map(|s| s.cores.iter().copied())
        .collect();
    cores.sort();
    cores.dedup();
    for core in cores {
        let found = match (&ra, env.core(core)) {
            (Some(r), Some(c)) => Some(format!("{} -L {}", describe(r), c.display())),
            _ => None,
        };
        out.push(EmulatorStatus {
            name: format!("retroarch:{core}"),
            systems: systems::SYSTEMS
                .iter()
                .filter(|s| s.cores.contains(&core))
                .map(|s| s.key.to_owned())
                .collect(),
            found,
        });
    }
    for e in KNOWN_EMULATORS {
        out.push(EmulatorStatus {
            name: e.name.to_owned(),
            systems: systems::SYSTEMS
                .iter()
                .filter(|s| s.emulators.contains(&e.name))
                .map(|s| s.key.to_owned())
                .collect(),
            found: env.known(e).map(|r| describe(&r)),
        });
    }
    Ok(out)
}
