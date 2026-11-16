// The total of the library and one row per platform: its games next to the biggest platform's.
// A platform's button shows its games on the Games tab.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    property var host
    readonly property var view: section.host ? section.host.overview : null

    spacing: 8

    QQC2.ProgressBar {
        indeterminate: true
        visible: section.host && section.host.loading && !section.view.loaded
        Layout.fillWidth: true
    }
    ErrorCard {
        title: "Couldn't read the library"
        detail: section.view ? section.view.problem : ""
        visible: section.view && section.view.problem !== ""
        Layout.fillWidth: true
    }
    SettingRow {
        settingId: "library"
        label: section.view && section.view.loaded ? section.view.summary : "Looking…"
        highlight: section.host ? section.host.highlight : ""
        visible: section.view && section.view.problem === ""
        Layout.fillWidth: true
    }
    UqLabel {
        text: section.view ? section.view.detail : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    UqLabel {
        text: section.host && section.view.needsAttention > 0 ? section.view.attentionText + " See Needs attention." : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    UqLabel {
        text: section.host ? section.host.openProblem : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    Repeater {
        model: section.view ? section.view.platforms : []

        SettingRow {
            required property var modelData
            settingId: "platforms"
            label: modelData.name
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            QQC2.ProgressBar {
                value: modelData.fraction
                Layout.preferredWidth: 120
            }
            UqLabel {
                text: modelData.gamesText + " · " + modelData.verifiedText
                opacity: 0.7
                Layout.preferredWidth: 150
                horizontalAlignment: Text.AlignRight
            }
            QQC2.Button {
                text: "View"
                onClicked: section.host.showPlatform(modelData.system)
            }
        }
    }
}
