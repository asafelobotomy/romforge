// Where romforge keeps and looks for games. Read-only: changing them is romforge's job.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    property var host

    spacing: 4

    Repeater {
        model: section.host ? section.host.overview.folders : []

        SettingRow {
            required property var modelData
            settingId: "folders"
            label: modelData.label
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            UqLabel {
                text: modelData.path !== "" ? modelData.path : "Not set"
                elide: Text.ElideMiddle
                opacity: 0.7
                Layout.maximumWidth: 360
            }
        }
    }
    QQC2.Button {
        text: "Change in romforge"
        onClicked: section.host.openApp()
    }
}
