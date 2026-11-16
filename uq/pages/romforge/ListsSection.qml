// Platforms you have games for that romforge has no game list for yet.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    property var host
    readonly property var rows: section.host ? section.host.overview.missingLists : []

    spacing: 4

    UqLabel {
        text: section.host && section.host.overview.loaded && section.rows.length === 0 ? "Every platform you have games for has its list." : ""
        visible: text !== ""
        opacity: 0.7
        Layout.fillWidth: true
    }
    Repeater {
        model: section.rows

        SettingRow {
            required property var modelData
            settingId: "lists"
            label: modelData.name
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            UqLabel {
                text: "No list yet"
                opacity: 0.7
            }
        }
    }
    QQC2.Button {
        text: "Open romforge"
        visible: section.rows.length > 0
        onClicked: section.host.openApp()
    }
}
