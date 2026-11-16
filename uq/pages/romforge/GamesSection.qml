// The Games tab: four choices (platform, which games, sort, words in the title) and the list
// they make, fifty games at a time. Read-only: a game is played or changed in romforge.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var platformChoices: Logic.platformChoices(section.host ? section.host.overview : null)

    spacing: 8

    Flow {
        spacing: 8
        Layout.fillWidth: true

        UqComboBox {
            objectName: "platform"
            model: Logic.labels(section.platformChoices)
            currentIndex: Logic.choiceIndex(section.platformChoices, section.host ? section.host.platform : "all")
            onActivated: index => {
                section.host.platform = Logic.choiceId(section.platformChoices, index);
                section.host.loadGames();
            }
        }
        UqComboBox {
            objectName: "status"
            model: Logic.labels(Logic.STATUS_CHOICES)
            currentIndex: Logic.choiceIndex(Logic.STATUS_CHOICES, section.host ? section.host.status : "all")
            onActivated: index => {
                section.host.status = Logic.choiceId(Logic.STATUS_CHOICES, index);
                section.host.loadGames();
            }
        }
        UqComboBox {
            objectName: "sort"
            model: Logic.labels(Logic.SORT_CHOICES)
            currentIndex: Logic.choiceIndex(Logic.SORT_CHOICES, section.host ? section.host.sort : "title")
            onActivated: index => {
                section.host.sort = Logic.choiceId(Logic.SORT_CHOICES, index);
                section.host.loadGames();
            }
        }
        QQC2.ToolButton {
            objectName: "direction"
            text: section.host && section.host.descending ? "Z to A" : "A to Z"
            onClicked: {
                section.host.descending = !section.host.descending;
                section.host.loadGames();
            }
        }
        QQC2.TextField {
            objectName: "search"
            placeholderText: "Search titles"
            text: section.host ? section.host.search : ""
            implicitWidth: 220
            onTextEdited: {
                section.host.search = text;
                typing.restart();
            }
            Timer {
                id: typing
                interval: 300
                onTriggered: section.host.loadGames()
            }
        }
    }

    ErrorCard {
        title: "Couldn't read the games"
        detail: section.host ? section.host.games.problem : ""
        visible: section.host && section.host.games.problem !== ""
        Layout.fillWidth: true
    }
    QQC2.ProgressBar {
        indeterminate: true
        visible: section.host && section.host.gamesLoading
        Layout.fillWidth: true
    }
    UqLabel {
        text: section.host ? section.host.games.shownText : ""
        visible: text !== "" && section.host.games.problem === ""
        opacity: 0.7
        Layout.fillWidth: true
    }
    Repeater {
        model: section.host ? section.host.games.rows : []

        SettingRow {
            required property var modelData
            settingId: "browse"
            label: modelData.title
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            UqLabel {
                text: modelData.subtitle
                elide: Text.ElideRight
                opacity: 0.7
                Layout.maximumWidth: 260
            }
            UqLabel {
                text: modelData.status
                Layout.preferredWidth: 110
            }
            UqLabel {
                text: modelData.size
                opacity: 0.7
                Layout.preferredWidth: 70
                horizontalAlignment: Text.AlignRight
            }
        }
    }
    QQC2.Button {
        objectName: "more"
        text: "Show more"
        visible: section.host && section.host.games.more
        enabled: section.host && !section.host.gamesLoading
        onClicked: section.host.moreGames()
    }
    QQC2.Button {
        text: "Open romforge"
        onClicked: section.host.openApp()
    }
}
