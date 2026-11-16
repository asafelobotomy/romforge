.pragma library
// The page's oysterAV logic: no Qt calls, so Node can test it (tests/uq/av.test.mjs). What
// `romforge-cli oysterav --ndjson` prints is untrusted: it is read defensively, lists are capped
// and strings are cut.

var TARGET_LABELS = { "emulators": "Emulators and cores", "games": "Games" };
var PATTERNS = {
    "oysterav": { "target": "^(emulators|games)$" },
    "oysterav-all": {},
    "scan-setting": { "value": "^(true|false)$" }
};
var FINDING_LIMIT = 20;
var RESULT_LIMIT = 40;

function has(table, key) {
    return typeof key === "string" && Object.prototype.hasOwnProperty.call(table, key);
}

function clip(value, limit) {
    var s = typeof value === "string" ? value : (value === null || value === undefined ? "" : String(value));
    s = s.replace(/[\u0000-\u001f\u007f]/g, " ");
    return s.length > limit ? s.slice(0, limit - 1) + "…" : s;
}

function plural(n, one, many) {
    return n + " " + (n === 1 ? one : many);
}

/// "" when the arguments fit the command's patterns; otherwise what is wrong.
function argsProblem(command, args) {
    if (!has(PATTERNS, command))
        return "Unknown request.";
    var names = Object.keys(PATTERNS[command]);
    if (!Array.isArray(args) || args.length !== names.length)
        return "Wrong number of arguments.";
    for (var i = 0; i < names.length; i++) {
        if (typeof args[i] !== "string" || !new RegExp(PATTERNS[command][names[i]]).test(args[i]))
            return "That choice can't be used here.";
    }
    return "";
}

function emptyRun() {
    return { started: false, places: [], results: [], threats: 0, errors: 0, finished: false };
}

/// A new look at oysterAV's answers: nothing yet.
function startRun() {
    var r = emptyRun();
    r.started = true;
    return r;
}

/// The next state after one event (`start`, `scanned` or `done`); anything else changes nothing.
function reduce(run, event) {
    if (!event || typeof event !== "object" || typeof event.event !== "string")
        return run;
    var next = JSON.parse(JSON.stringify(run));
    if (event.event === "start") {
        next.places = Array.isArray(event.targets) ? event.targets.slice(0, RESULT_LIMIT).map(function (t) { return clip(t, 120); }) : [];
    } else if (event.event === "scanned") {
        var state = event.state === "clean" || event.state === "threat" ? event.state : "error";
        var findings = [];
        var raw = Array.isArray(event.findings) ? event.findings : [];
        for (var i = 0; i < raw.length && i < FINDING_LIMIT; i++) {
            if (raw[i] && typeof raw[i] === "object")
                findings.push({ path: clip(raw[i].path, 300), threat: clip(raw[i].threat, 120) });
        }
        if (next.results.length < RESULT_LIMIT) {
            next.results.push({
                name: has(TARGET_LABELS, event.target) ? TARGET_LABELS[event.target] : clip(event.target, 120),
                state: state,
                detail: clip(event.detail, 300),
                findings: findings,
                more: Array.isArray(event.findings) ? Math.max(0, event.findings.length - FINDING_LIMIT) : 0
            });
        }
    } else if (event.event === "done") {
        next.finished = true;
        next.threats = typeof event.threats === "number" ? event.threats : 0;
        next.errors = typeof event.errors === "number" ? event.errors : 0;
    }
    return next;
}

/// What a finished (or running) look says, in words.
function summary(run) {
    if (!run || !run.started)
        return "";
    if (!run.finished)
        return run.results.length === 0 ? "oysterAV is looking…" : "oysterAV is looking… " + plural(run.results.length, "place", "places") + " done.";
    if (run.results.length === 0)
        return "Nothing to look at yet: there is no RomForge or Library folder.";
    if (run.threats > 0)
        return "oysterAV found " + plural(run.threats, "threat", "threats") + ". Don't open those files; see oysterAV for what to do.";
    if (run.errors > 0)
        return "Clean so far, but " + plural(run.errors, "place", "places") + " couldn't be checked.";
    return "All clean.";
}

/// The line under one place's name.
function resultText(r) {
    if (r.state === "clean")
        return "Clean";
    if (r.state === "threat")
        return "Found " + (r.detail || "a threat");
    return "Couldn't be checked" + (r.detail ? ": " + r.detail : "");
}

/// "" when the stream ended well; otherwise why the page should say it failed.
function streamProblem(run, result) {
    if (run && run.finished)
        return "";
    var why = result && typeof result.error === "string" && result.error !== "" ? result.error : "romforge-cli gave no answer";
    return clip(why, 300);
}
