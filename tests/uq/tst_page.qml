// The Game library page, offscreen, with the stub org.uq.settings module and a fake ctx:
//   QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
// The answers are real `romforge-cli` output (tests/uq/fixtures).
import QtQuick
import QtQuick.Controls as QQC2
import QtTest

Item {
    id: root
    width: 900
    height: 700

    readonly property url pageUrl: Qt.resolvedUrl("../../uq/pages/romforge/Page.qml")

    function fixture(name) {
        const xhr = new XMLHttpRequest();
        xhr.open("GET", Qt.resolvedUrl("fixtures/" + name + ".json"), false);
        xhr.send();
        return JSON.parse(xhr.responseText);
    }

    FakeContext {
        id: fake
        confirmed: []
    }

    Loader {
        id: loader
        anchors.fill: parent
    }

    function find(item, match) {
        if (match(item))
            return item;
        for (let i = 0; i < item.children.length; ++i) {
            const hit = find(item.children[i], match);
            if (hit)
                return hit;
        }
        return null;
    }
    function findLabel(text) {
        return find(loader.item, item => item.text === text);
    }
    function findByName(name) {
        return find(loader.item, item => item.objectName === name);
    }
    function browseCalls() {
        return fake.calls.filter(c => c[0] === "run" && c[1] === "browse");
    }

    TestCase {
        name: "GameLibraryPage"
        when: windowShown

        function answers() {
            return {
                overview: { code: 0, output: root.fixture("overview") },
                browse: args => ({ code: 0, output: args[4] === "0" ? root.fixture("browse") : { games: [{ id: "g:1:Next", title: "Next one", status: "verified", size: 5, system: "NES", system_name: "Nintendo Entertainment System", versions: 1 }], total: 13 } })
            };
        }

        function init() {
            loader.source = "";
            fake.calls = [];
            fake.watches = ({});
            fake.commands = answers();
            loader.source = root.pageUrl;
            verify(loader.status === Loader.Ready, "the page loads");
            loader.item.ctx = fake.ctx;
        }

        function test_reads_the_library_and_the_first_games_on_open() {
            tryVerify(() => root.findLabel("6 games · 7.2 kB") !== null || loader.item.overview.loaded);
            compare(fake.calls[0], ["watch", "overview"]);
            tryVerify(() => loader.item.games.loaded);
            compare(root.browseCalls()[0][2], ["all", "all", "title", "50", "0", "*"]);
            verify(root.findLabel("Nintendo Entertainment System") !== null);
            verify(root.findLabel("Showing 3 of 13") !== null);
        }

        function test_a_change_to_the_library_reads_it_again() {
            tryVerify(() => loader.item.overview.loaded);
            const before = fake.calls.length;
            const out = root.fixture("overview");
            out.totals.games = 7;
            fake.commands = ({ overview: { code: 0, output: out }, browse: answers().browse });
            fake.change("overview");
            tryVerify(() => loader.item.overview.summary.indexOf("7 games") === 0);
            verify(fake.calls.length > before);
        }

        function test_a_failed_read_shows_the_reason() {
            fake.commands = ({ overview: { error: "timed out" }, browse: answers().browse });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("timed out") !== null);
            verify(root.findLabel("Couldn't read the library") !== null);
        }

        function test_a_platform_button_shows_its_games() {
            tryVerify(() => loader.item.overview.loaded);
            const view = root.find(loader.item, item => item.text === "View");
            verify(view !== null);
            fake.calls = [];
            view.clicked();
            tryVerify(() => root.browseCalls().length === 1);
            compare(root.browseCalls()[0][2][0], loader.item.overview.platforms[0].system);
            compare(root.browseCalls()[0][2][1], "all");
        }

        function test_the_choices_make_the_arguments() {
            tryVerify(() => loader.item.games.loaded);
            fake.calls = [];
            loader.item.status = "verified";
            loader.item.sort = "year";
            loader.item.descending = true;
            loader.item.loadGames();
            tryVerify(() => root.browseCalls().length === 1);
            compare(root.browseCalls()[0][2], ["all", "verified", "year:desc", "50", "0", "*"]);
        }

        function test_typing_asks_once_after_a_pause() {
            tryVerify(() => loader.item.games.loaded);
            const field = root.findByName("search");
            verify(field !== null);
            fake.calls = [];
            field.text = "zel";
            field.textEdited();
            field.text = "zelda";
            field.textEdited();
            tryVerify(() => root.browseCalls().length === 1, 2000);
            compare(root.browseCalls()[0][2][5], "zelda");
            wait(500);
            compare(root.browseCalls().length, 1);
        }

        function test_show_more_adds_the_next_fifty() {
            tryVerify(() => loader.item.games.loaded);
            fake.calls = [];
            loader.item.moreGames();
            tryVerify(() => loader.item.games.rows.length === 4);
            compare(root.browseCalls()[0][2][4], "3");
            verify(root.findLabel("Next one") !== null);
        }

        function test_an_answer_to_an_older_choice_is_dropped() {
            tryVerify(() => loader.item.games.loaded);
            const late = [];
            fake.commands = ({ overview: answers().overview, browse: args => ({ code: 0, output: { games: [{ id: "x", title: args[0] + " game", status: "verified", size: 1 }], total: 1 } }) });
            loader.item.platform = "NES";
            loader.item.loadGames();
            loader.item.platform = "SNES";
            loader.item.loadGames();
            tryVerify(() => loader.item.games.rows.length === 1 && loader.item.games.rows[0].title === "SNES game");
            wait(100);
            compare(loader.item.games.rows[0].title, "SNES game");
        }

        function test_open_romforge_opens_the_app() {
            tryVerify(() => loader.item.overview.loaded);
            const button = root.find(loader.item, item => item.text === "Open romforge");
            verify(button !== null);
            fake.calls = [];
            button.clicked();
            compare(fake.calls[0], ["openApp", "romforge.desktop"]);
        }

        function test_an_app_that_cannot_open_says_so() {
            tryVerify(() => loader.item.overview.loaded);
            loader.item.ctx = Object.assign({}, fake.ctx, { openApp: (id, callback) => callback({ error: "no such app" }) });
            loader.item.openApp();
            tryVerify(() => root.findLabel("romforge couldn't be opened. Is it installed?") !== null);
        }

        function test_what_needs_attention_is_listed() {
            tryVerify(() => loader.item.overview.loaded);
            verify(root.findLabel("DuckStation needs a BIOS file") !== null);
            verify(root.findLabel("Atari 7800 ProSystem") !== null);
            verify(root.findLabel("6 things need attention. See Needs attention.") !== null);
        }

        function test_an_empty_library_says_how_to_start() {
            fake.commands = ({ overview: { code: 0, output: root.fixture("overview-empty") }, browse: { code: 0, output: root.fixture("browse-empty") } });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("No games yet") !== null);
            tryVerify(() => root.findLabel("No games match.") !== null);
            verify(root.findLabel("No BIOS files are missing.") !== null);
        }

        function test_it_reads_until_asked_to_look() {
            tryVerify(() => loader.item.games.loaded);
            for (const c of fake.calls)
                verify(c[0] === "watch" || c[1] === "overview" || c[1] === "browse", JSON.stringify(c));
        }

        function test_check_games_streams_one_look_and_shows_the_threat() {
            tryVerify(() => loader.item.overview.loaded);
            fake.events = ({ oysterav: [
                    { event: "start", targets: ["games"] },
                    { event: "scanned", target: "games", state: "threat", detail: "Bad.One", findings: [{ path: "/g/a.zip", threat: "Bad.One" }] },
                    { event: "done", clean: false, threats: 1, errors: 0 }
                ] });
            fake.commands = Object.assign({}, fake.commands, { oysterav: { code: 1, output: null } });
            fake.calls = [];
            const button = root.findByName("check-games");
            verify(button !== null);
            button.clicked();
            compare(fake.calls[0], ["stream", "oysterav", ["games"]]);
            tryVerify(() => root.findLabel("Found Bad.One") !== null);
            verify(root.findLabel("/g/a.zip  —  Bad.One") !== null);
            verify(root.findByName("av-summary").text.indexOf("found 1 threat") >= 0);
            tryVerify(() => !loader.item.looking);
            compare(loader.item.avProblem, "");
        }

        function test_check_everything_asks_for_both() {
            tryVerify(() => loader.item.overview.loaded);
            fake.events = ({ "oysterav-all": [
                    { event: "start", targets: ["emulators", "games"] },
                    { event: "scanned", target: "emulators", state: "clean", detail: "", findings: [] },
                    { event: "scanned", target: "games", state: "clean", detail: "", findings: [] },
                    { event: "done", clean: true, threats: 0, errors: 0 }
                ] });
            fake.commands = Object.assign({}, fake.commands, { "oysterav-all": { code: 0, output: null } });
            fake.calls = [];
            root.findByName("check-all").clicked();
            compare(fake.calls[0], ["stream", "oysterav-all", []]);
            tryVerify(() => root.findByName("av-summary").text === "All clean.");
        }

        function test_a_look_that_cannot_start_says_why() {
            tryVerify(() => loader.item.overview.loaded);
            fake.commands = Object.assign({}, fake.commands, { oysterav: { code: 1, error: "no RomForge folder or Library folder to look at yet" } });
            root.findByName("check-emulators").clicked();
            tryVerify(() => loader.item.avProblem !== "");
            verify(root.findLabel("no RomForge folder or Library folder to look at yet") !== null);
        }

        function test_the_switch_saves_the_setting_and_reads_the_overview_again() {
            tryVerify(() => loader.item.overview.loaded);
            fake.commands = Object.assign({}, fake.commands, { "scan-setting": { code: 0, output: null } });
            const flipped = root.fixture("overview");
            flipped.scan_downloads = true;
            fake.calls = [];
            const toggle = root.findByName("scan-downloads");
            verify(toggle !== null);
            compare(toggle.value, false);
            fake.commands.overview = { code: 0, output: flipped };
            toggle.chosen(true);
            compare(fake.calls[0], ["run", "scan-setting", ["true"]]);
            tryVerify(() => loader.item.overview.scanDownloads === true);
            tryVerify(() => root.findByName("scan-downloads").value === true);
        }
    }
}
