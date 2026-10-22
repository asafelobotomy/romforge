//! romforge's Steam Input template for the Steam Deck (`controller_neptune`),
//! installed into Steam's `controller_base/templates/` (where Steam lists
//! templates; EmuDeck installs its own there). Laid out after the
//! structure of Valve's "Gamepad with Mouse Trackpad": an Xbox pad, since
//! romforge reads the pad itself, the right trackpad as a mouse, and the
//! back buttons as romforge's hotkeys: L4 Select+Start (quit), R4 L3 (hold
//! it for the radial menu), L5 Select, R5 Start. Several `binding` lines in
//! one activator press their buttons together.

/// The file's name in `controller_base/templates/`.
pub const FILE: &str = "romforge_steamdeck.vdf";

fn press(input: &str, buttons: &[&str]) -> String {
    let lines: String = buttons
        .iter()
        .map(|b| format!("\t\t\t\t\t\t\t\"binding\"\t\t\"xinput_button {b}, , \"\n"))
        .collect();
    format!(
        "\t\t\t\"{input}\"\n\t\t\t{{\n\t\t\t\t\"activators\"\n\t\t\t\t{{\n\t\t\t\t\t\"Full_Press\"\n\t\t\t\t\t{{\n\t\t\t\t\t\t\"bindings\"\n\t\t\t\t\t\t{{\n{lines}\t\t\t\t\t\t}}\n\t\t\t\t\t}}\n\t\t\t\t}}\n\t\t\t}}\n"
    )
}

fn group(id: u32, mode: &str, inputs: &[String], settings: &str) -> String {
    format!(
        "\t\"group\"\n\t{{\n\t\t\"id\"\t\t\"{id}\"\n\t\t\"mode\"\t\t\"{mode}\"\n\t\t\"inputs\"\n\t\t{{\n{}\t\t}}\n{settings}\t}}\n",
        inputs.concat()
    )
}

/// The template's text.
pub fn text() -> String {
    let mut out = String::from(
        "\"controller_mappings\"\n{\n\t\"version\"\t\t\"3\"\n\t\"revision\"\t\t\"1\"\n\t\"title\"\t\t\"romforge (Deck)\"\n\t\"description\"\t\t\"romforge's layout: a gamepad (romforge reads it itself), the right trackpad as mouse, L4 quits a game (Select+Start), R4 held opens the radial menu (L3), L5 Select, R5 Start.\"\n\t\"creator\"\t\t\"\"\n\t\"progenitor\"\t\t\"\"\n\t\"url\"\t\t\"\"\n\t\"export_type\"\t\t\"\"\n\t\"controller_type\"\t\t\"controller_neptune\"\n\t\"major_revision\"\t\t\"1\"\n\t\"minor_revision\"\t\t\"0\"\n\t\"actions\"\n\t{\n\t\t\"Preset_1000001\"\n\t\t{\n\t\t\t\"title\"\t\t\"Default\"\n\t\t\t\"legacy_set\"\t\t\"1\"\n\t\t}\n\t}\n\t\"action_layers\"\n\t{\n\t}\n",
    );
    out += &group(
        0,
        "four_buttons",
        &[
            press("button_a", &["A"]),
            press("button_b", &["B"]),
            press("button_x", &["X"]),
            press("button_y", &["Y"]),
        ],
        "",
    );
    out += &group(
        1,
        "dpad",
        &[
            press("dpad_north", &["DPAD_UP"]),
            press("dpad_south", &["DPAD_DOWN"]),
            press("dpad_east", &["DPAD_RIGHT"]),
            press("dpad_west", &["DPAD_LEFT"]),
        ],
        "",
    );
    out += &group(2, "joystick_move", &[press("click", &["JOYSTICK_LEFT"])], "");
    out += &group(3, "joystick_move", &[press("click", &["JOYSTICK_RIGHT"])], "");
    out += &group(
        4,
        "trigger",
        &[],
        "\t\t\"settings\"\n\t\t{\n\t\t\t\"output_trigger\"\t\t\"1\"\n\t\t}\n",
    );
    out += &group(
        5,
        "trigger",
        &[],
        "\t\t\"settings\"\n\t\t{\n\t\t\t\"output_trigger\"\t\t\"2\"\n\t\t}\n",
    );
    out += &group(6, "absolute_mouse", &[], "");
    out += &group(
        7,
        "switches",
        &[
            press("button_escape", &["START"]),
            press("button_menu", &["SELECT"]),
            press("left_bumper", &["SHOULDER_LEFT"]),
            press("right_bumper", &["SHOULDER_RIGHT"]),
            press("button_back_left_upper", &["SELECT", "START"]),
            press("button_back_right_upper", &["JOYSTICK_LEFT"]),
            press("button_back_left", &["SELECT"]),
            press("button_back_right", &["START"]),
        ],
        "",
    );
    out += "\t\"preset\"\n\t{\n\t\t\"id\"\t\t\"0\"\n\t\t\"name\"\t\t\"Preset_1000001\"\n\t\t\"group_source_bindings\"\n\t\t{\n";
    for (id, source) in [
        (0, "button_diamond"),
        (1, "dpad"),
        (2, "joystick"),
        (3, "right_joystick"),
        (4, "left_trigger"),
        (5, "right_trigger"),
        (6, "right_trackpad"),
        (7, "switch"),
    ] {
        out += &format!("\t\t\t\"{id}\"\t\t\"{source} active\"\n");
    }
    out += "\t\t}\n\t}\n\t\"settings\"\n\t{\n\t\t\"left_trackpad_mode\"\t\t\"0\"\n\t\t\"right_trackpad_mode\"\t\t\"0\"\n\t}\n}\n";
    out
}
