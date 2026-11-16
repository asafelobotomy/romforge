.pragma library
// The Game library page's pure logic: no Qt calls, so Node can test it (tests/uq/logic.test.mjs).
// Everything romforge-cli prints is untrusted: it is read defensively, a wrong type becomes an
// empty or "couldn't read" result, lists are capped and strings are cut.

/// The app opened by the page's buttons (the desktop file the romforge packages install).
var APP = "romforge.desktop";

var ROW_LIMIT = 500;
var NAME_LIMIT = 120;
var MESSAGE_LIMIT = 300;
/// Games fetched at a time in the list.
var PAGE_SIZE = 50;

var STATUS_CHOICES = [
    { id: "all", label: "All games" },
    { id: "verified", label: "Verified" },
    { id: "incomplete", label: "Incomplete" },
    { id: "unidentified", label: "Not identified" }
];
var SORT_CHOICES = [
    { id: "title", label: "Title" },
    { id: "platform", label: "Platform" },
    { id: "year", label: "Year" },
    { id: "status", label: "Status" }
];

var STATUS_TEXT = {
    "verified": "Verified",
    "incomplete": "Incomplete",
    "candidate": "Might be a game",
    "unknown": "Not identified",
    "wrong_system": "Another platform",
    "duplicate": "Copy",
    "by_you": "Named by you",
    "derivative": "Translation or hack",
    "detected": "Detected"
};

/// The argument patterns of page.toml's `browse` command: argument -> pattern. tests/uq checks
/// that they are the same strings.
var PATTERNS = {
    "browse": {
        "platform": "^(all|[A-Za-z0-9][A-Za-z0-9 '.-]{0,39})$",
        "status": "^(all|verified|incomplete|unidentified)$",
        "sort": "^(title|platform|year|status)(:desc)?$",
        "limit": "^[0-9]{1,4}$",
        "offset": "^[0-9]{1,7}$",
        "search": "^[^\\x00-\\x1f\\x7f-][^\\x00-\\x1f\\x7f]{0,79}$"
    }
};
var BROWSE_ORDER = ["platform", "status", "sort", "limit", "offset", "search"];

function has(table, key) {
    return typeof key === "string" && Object.prototype.hasOwnProperty.call(table, key);
}

/// A string for display: control characters become spaces, and it is cut to `limit`.
function clip(value, limit) {
    var s = typeof value === "string" ? value : (value === null || value === undefined ? "" : String(value));
    s = s.replace(/[\u0000-\u001f\u007f]/g, " ");
    var n = limit || NAME_LIMIT;
    return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function number(value) {
    return typeof value === "number" && isFinite(value) && value >= 0 ? value : null;
}

function plural(n, one, many) {
    return n + " " + (n === 1 ? one : many);
}

/// 1234 -> "1,234".
function count(n) {
    var v = number(n);
    if (v === null)
        return "0";
    return String(Math.round(v)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/// "1.2 GB": base 1000, one decimal, "—" when unknown.
function size(bytes) {
    var n = number(bytes);
    if (n === null)
        return "—";
    if (n < 1000)
        return Math.round(n) + " B";
    var units = ["kB", "MB", "GB", "TB", "PB"];
    var i = -1;
    do {
        n = n / 1000;
        i++;
    } while (n >= 999.95 && i < units.length - 1);
    return n.toFixed(1) + " " + units[i];
}

function isCancelled(result) {
    return !!(result && result.cancelled);
}

/// The data of a read, or why there is none. `output` is read for every exit code.
function read(result) {
    var r = result && typeof result === "object" ? result : {};
    var out = r.output;
    var data = out && typeof out === "object" && !Array.isArray(out) ? out : null;
    if (r.code === 0 && data !== null)
        return { ok: true, data: data, problem: "" };
    var why = typeof r.error === "string" && r.error !== "" ? r.error : "romforge-cli gave no answer";
    return { ok: false, data: null, problem: clip(why, MESSAGE_LIMIT) };
}

function text(value, limit) {
    return typeof value === "string" ? clip(value, limit) : "";
}

function list(value) {
    return Array.isArray(value) ? value.slice(0, ROW_LIMIT) : [];
}

function statusText(status) {
    return has(STATUS_TEXT, status) ? STATUS_TEXT[status] : "";
}

/// "2026-11-16T15:16:09Z" -> "2026-11-16"; "" when it isn't a date.
function day(value) {
    var m = /^(\d{4}-\d{2}-\d{2})/.exec(typeof value === "string" ? value : "");
    return m ? m[1] : "";
}

/// One game of `games` or `recent`, ready to show.
function gameRow(g) {
    if (!g || typeof g !== "object" || typeof g.title !== "string")
        return null;
    var year = number(g.year);
    var bits = [text(g.system_name || g.system, 60)];
    if (year !== null && year > 0)
        bits.push(String(Math.round(year)));
    var versions = number(g.versions);
    if (versions !== null && versions > 1)
        bits.push(plural(versions, "version", "versions"));
    return {
        id: text(g.id, 200),
        title: clip(g.title),
        subtitle: bits.filter(function (b) { return b !== ""; }).join(" · "),
        status: statusText(g.status),
        verified: g.status === "verified",
        size: size(g.size),
        added: day(g.added)
    };
}

function emptyOverview() {
    return {
        problem: "", loaded: false, hasGames: false, summary: "", detail: "", platforms: [],
        recent: [], bios: [], missingLists: [], folders: [], needsAttention: 0, attentionText: ""
    };
}

/// What the Library and Needs attention tabs show, from an `overview` answer.
function overviewView(result) {
    var view = emptyOverview();
    var r = read(result);
    if (!r.ok) {
        view.problem = r.problem;
        return view;
    }
    var d = r.data;
    var t = d.totals && typeof d.totals === "object" ? d.totals : {};
    var games = number(t.games) || 0;
    view.loaded = true;
    view.hasGames = games > 0;
    var unidentified = number(t.unidentified) || 0;
    if (games > 0) {
        view.summary = plural(games, "game", "games") + " · " + size(t.size);
        var parts = [count(t.verified) + " verified"];
        if ((number(t.incomplete) || 0) > 0)
            parts.push(count(t.incomplete) + " incomplete");
        if (unidentified > 0)
            parts.push(count(unidentified) + " files not identified");
        view.detail = parts.join(" · ");
    } else {
        view.summary = unidentified > 0 ? "No games identified yet" : "No games yet";
        view.detail = unidentified > 0
            ? plural(unidentified, "file", "files") + " found, none matched a game list. Open romforge to download the lists."
            : "romforge hasn't scanned any games yet. Open it to add a folder.";
    }
    var biggest = 1;
    var plats = list(d.platforms);
    for (var i = 0; i < plats.length; i++) {
        var g = plats[i] && number(plats[i].games);
        if (g !== null && g > biggest)
            biggest = g;
    }
    for (var j = 0; j < plats.length; j++) {
        var p = plats[j];
        if (!p || typeof p !== "object" || typeof p.system !== "string" || typeof p.name !== "string")
            continue;
        var n = number(p.games) || 0;
        var v = number(p.verified) || 0;
        view.platforms.push({
            system: clip(p.system, 40),
            name: clip(p.name, 80),
            short: text(p.short, 6),
            maker: text(p.manufacturer, 40),
            games: n,
            gamesText: plural(n, "game", "games"),
            verifiedText: v === n ? "all verified" : v + " verified",
            fraction: n / biggest
        });
    }
    var recent = list(d.recent);
    for (var k = 0; k < recent.length; k++) {
        var row = gameRow(recent[k]);
        if (row)
            view.recent.push(row);
    }
    var bios = list(d.bios);
    for (var b = 0; b < bios.length; b++) {
        var e = bios[b];
        if (e && typeof e.name === "string")
            view.bios.push({ name: clip(e.name, 80), emulator: text(e.emulator, 80) });
    }
    var lists = d.lists && typeof d.lists === "object" ? d.lists : {};
    var missing = list(lists.missing);
    for (var m = 0; m < missing.length; m++) {
        if (missing[m] && typeof missing[m].name === "string")
            view.missingLists.push({ name: clip(missing[m].name, 80) });
    }
    var folders = [["RomForge folder", d.home], ["Source folder", d.source], ["Library folder", d.library]];
    for (var f = 0; f < folders.length; f++)
        view.folders.push({ label: folders[f][0], path: typeof folders[f][1] === "string" ? clip(folders[f][1], 300) : "" });
    view.needsAttention = view.bios.length + view.missingLists.length;
    view.attentionText = view.needsAttention === 0 ? "Nothing is missing." : plural(view.needsAttention, "thing needs", "things need") + " attention.";
    return view;
}

function emptyGames() {
    return { problem: "", loaded: false, total: 0, rows: [], shownText: "", more: false };
}

/// The rows of a `browse` answer. `previous` is the rows already shown when it is a next page.
function gamesView(result, previous) {
    var view = emptyGames();
    var r = read(result);
    if (!r.ok) {
        view.problem = r.problem;
        view.rows = previous || [];
        return view;
    }
    view.loaded = true;
    view.total = number(r.data.total) || 0;
    var rows = (previous || []).slice();
    var games = list(r.data.games);
    for (var i = 0; i < games.length; i++) {
        var row = gameRow(games[i]);
        if (row)
            rows.push(row);
    }
    view.rows = rows;
    view.more = rows.length < view.total;
    view.shownText = view.total === 0 ? "No games match." : (rows.length >= view.total ? plural(view.total, "game", "games") : "Showing " + count(rows.length) + " of " + count(view.total));
    return view;
}

/// The platform choices of the Games tab: "All platforms", then one per platform of `overview`.
function platformChoices(overview) {
    var out = [{ id: "all", label: "All platforms" }];
    var plats = overview && Array.isArray(overview.platforms) ? overview.platforms : [];
    for (var i = 0; i < plats.length; i++)
        out.push({ id: plats[i].system, label: plats[i].name });
    return out;
}

/// The id of choice number `index`, or the first when it is out of range.
function choiceId(choices, index) {
    return choices[index >= 0 && index < choices.length ? index : 0].id;
}

function choiceIndex(choices, id) {
    for (var i = 0; i < choices.length; i++)
        if (choices[i].id === id)
            return i;
    return 0;
}

function labels(choices) {
    return choices.map(function (c) { return c.label; });
}

/// The arguments of `browse` for the person's choices. `search` has its leading dashes taken
/// off (uQ refuses an argument that starts with one) and `*` stands for no search.
function browseArgs(platform, status, sort, descending, offset, search) {
    var words = typeof search === "string" ? search.replace(/[\u0000-\u001f\u007f]/g, " ").trim().replace(/^-+/, "").trim().slice(0, 80) : "";
    return [
        platform,
        status,
        sort + (descending ? ":desc" : ""),
        String(PAGE_SIZE),
        String(Math.max(0, offset || 0)),
        words === "" ? "*" : words
    ];
}

/// "" when every argument matches its pattern; otherwise what is wrong (the page then asks
/// nothing). uQ checks the same patterns again.
function argsProblem(command, args) {
    if (!has(PATTERNS, command))
        return "Unknown request.";
    var order = BROWSE_ORDER;
    if (!Array.isArray(args) || args.length !== order.length)
        return "Wrong number of arguments.";
    for (var i = 0; i < order.length; i++) {
        if (typeof args[i] !== "string" || !new RegExp(PATTERNS[command][order[i]]).test(args[i]))
            return "That choice can't be used here.";
    }
    return "";
}
