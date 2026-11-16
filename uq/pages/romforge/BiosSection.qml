// Platforms whose emulator can't start games for want of a BIOS file.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    property var host
    readonly property var rows: section.host ? section.host.overview.bios : []

    spacing: 4

    UqLabel {
        text: section.host && section.host.overview.loaded && section.rows.length === 0 ? "No BIOS files are missing." : ""
        visible: text !== ""
        opacity: 0.7
        Layout.fillWidth: true
    }
    Repeater {
        model: section.rows

        SettingRow {
            required property var modelData
            settingId: "bios"
            label: modelData.name
            highlight: section.host ? section.host.highlight : ""
            Layout.fillWidth: true

            UqLabel {
                text: modelData.emulator + " needs a BIOS file"
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
