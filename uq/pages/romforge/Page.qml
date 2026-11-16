// Game library: a read-only look at what romforge has found. Everything is read through
// `romforge-cli` (page.toml); logic.js reads its answers. Nothing here changes a game: the
// buttons open romforge itself, which does the rest. Laid out like Settings' own pages
// (uq_api = 2): three tabs of cards.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

SettingsPage {
    id: page

    property var ctx
    property var theme
    /// The [[setting]] row Settings search jumped to; the rows flash, and their tab opens.
    property string highlight

    /// What `overview` last said, and whether it is being read.
    property var overview: Logic.emptyOverview()
    property bool loading: true
    /// The list on the Games tab, and the choices that make it.
    property var games: Logic.emptyGames()
    property bool gamesLoading: false
    property string platform: "all"
    property string status: "all"
    property string sort: "title"
    property bool descending: false
    property string search: ""
    /// Counts the lists asked for, so an answer to an older choice is dropped.
    property int generation: 0
    /// Why romforge couldn't be opened, "" when it could.
    property string openProblem: ""
    property var openedWith: null
    property var watching: null

    pageId: "romforge"

    function openApp() {
        if (!page.ctx)
            return;
        page.openProblem = "";
        page.ctx.openApp(Logic.APP, result => {
            if (page && result && result.error)
                page.openProblem = "romforge couldn't be opened. Is it installed?";
        });
    }

    /// Reads the first page of the list again with the current choices.
    function loadGames() {
        page.fetchGames(0);
    }

    function fetchGames(offset) {
        if (!page.ctx)
            return;
        const args = Logic.browseArgs(page.platform, page.status, page.sort, page.descending, offset, page.search);
        const why = Logic.argsProblem("browse", args);
        if (why !== "") {
            page.games = Object.assign(Logic.emptyGames(), { problem: why });
            return;
        }
        const mine = ++page.generation;
        const before = offset > 0 ? page.games.rows : [];
        page.gamesLoading = true;
        page.ctx.command.run("browse", args, result => {
            if (!page || mine !== page.generation)
                return;
            page.gamesLoading = false;
            if (!Logic.isCancelled(result))
                page.games = Logic.gamesView(result, before);
        });
    }

    function moreGames() {
        if (!page.gamesLoading && page.games.more)
            page.fetchGames(page.games.rows.length);
    }

    /// Shows the games of one platform on the Games tab.
    function showPlatform(system) {
        page.platform = system;
        page.status = "all";
        page.search = "";
        page.loadGames();
        gamesTab.activate();
    }

    // `overview` runs now and again whenever romforge's library file changes.
    onCtxChanged: {
        if (page.ctx && page.ctx !== page.openedWith) {
            page.openedWith = page.ctx;
            page.watching = page.ctx.command.watch("overview", result => {
                if (!page)
                    return;
                page.loading = false;
                if (Logic.isCancelled(result))
                    return;
                const first = !page.overview.loaded;
                page.overview = Logic.overviewView(result);
                // The list is read once the library has answered, and again when it changed.
                if (first || page.overview.loaded)
                    page.loadGames();
            });
        }
    }

    Component.onDestruction: {
        if (page.ctx && page.watching !== null)
            page.ctx.command.unwatch(page.watching);
    }

    SettingsTab {
        title: "Library"
        iconName: "applications-games"

        SettingsCard {
            title: "Your library"
            iconName: "folder-games"
            help: "What romforge has found. This page only looks: romforge itself adds, tidies and plays games."
            wide: true
            Layout.fillWidth: true
            actions: [
                QQC2.Button {
                    text: "Refresh"
                    enabled: !page.loading
                    onClicked: {
                        page.loading = true;
                        page.ctx.command.run("overview", [], result => {
                            if (!page)
                                return;
                            page.loading = false;
                            if (!Logic.isCancelled(result))
                                page.overview = Logic.overviewView(result);
                        });
                    }
                },
                QQC2.Button {
                    text: "Open romforge"
                    highlighted: true
                    onClicked: page.openApp()
                }
            ]

            LibrarySection {
                host: page
                Layout.fillWidth: true
            }
        }
        SettingsCard {
            title: "Recently added"
            iconName: "document-open-recent"
            help: "The games whose files were scanned most recently."
            Layout.fillWidth: true
            visible: page.overview.recent.length > 0

            RecentSection {
                host: page
                Layout.fillWidth: true
            }
        }
        SettingsCard {
            title: "Folders"
            iconName: "folder"
            help: "Where romforge keeps and looks for games. Change them in romforge."
            Layout.fillWidth: true
            visible: page.overview.loaded

            FoldersSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        id: gamesTab
        title: "Games"
        iconName: "view-list-details"

        SettingsCard {
            title: "Browse games"
            iconName: "view-list-details"
            help: "Pick a platform or type part of a title. To play or change a game, open romforge."
            wide: true
            Layout.fillWidth: true

            GamesSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Needs attention"
        iconName: "dialog-warning"

        SettingsCard {
            title: "BIOS files"
            iconName: "cpu"
            help: "Some emulators can't start a platform's games without its BIOS files."
            Layout.fillWidth: true

            BiosSection {
                host: page
                Layout.fillWidth: true
            }
        }
        SettingsCard {
            title: "Game lists"
            iconName: "view-list-text"
            help: "romforge names games by checking them against a list for their platform."
            Layout.fillWidth: true

            ListsSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }
}
