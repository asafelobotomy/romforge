// The games whose files were scanned last.
import QtQuick
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    property var host

    spacing: 4

    Repeater {
        model: section.host ? section.host.overview.recent : []

        SettingRow {
            required property var modelData
            settingId: "recent"
            label: modelData.title
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            UqLabel {
                text: modelData.subtitle + (modelData.added !== "" ? " · " + modelData.added : "")
                opacity: 0.7
            }
        }
    }
}
