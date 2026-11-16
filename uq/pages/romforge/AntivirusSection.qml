// oysterAV: look at what romforge keeps, and whether it looks at what it downloads.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "av.js" as Av

ColumnLayout {
    id: section

    property var host
    readonly property bool idle: !!section.host && !section.host.looking

    spacing: 6

    SwitchRow {
        objectName: "scan-downloads"
        settingId: "oysterav"
        label: "Check downloads with oysterAV"
        highlight: section.host ? section.host.highlight : ""
        enabled: section.idle
        value: !!section.host && section.host.overview.scanDownloads
        onChosen: v => section.host.setScanDownloads(v)
        Layout.fillWidth: true
    }
    UqLabel {
        text: "Each emulator and core romforge downloads is looked at before it is put in place. One with a threat in it, or that oysterAV couldn't check, is not installed."
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    RowLayout {
        spacing: 8
        QQC2.Button {
            objectName: "check-all"
            text: "Check everything"
            enabled: section.idle
            onClicked: section.host.checkWithOysterAv("")
        }
        QQC2.Button {
            objectName: "check-emulators"
            text: "Check emulators"
            enabled: section.idle
            onClicked: section.host.checkWithOysterAv("emulators")
        }
        QQC2.Button {
            objectName: "check-games"
            text: "Check games"
            enabled: section.idle
            onClicked: section.host.checkWithOysterAv("games")
        }
    }
    UqLabel {
        objectName: "av-summary"
        text: section.host ? Av.summary(section.host.look) : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    Repeater {
        model: section.host ? section.host.look.results : []

        ColumnLayout {
            required property var modelData
            spacing: 2
            Layout.fillWidth: true

            SettingRow {
                settingId: "oysterav"
                label: modelData.name
                highlight: section.host ? section.host.highlight : ""
                Layout.fillWidth: true

                UqLabel {
                    text: Av.resultText(modelData)
                    opacity: 0.7
                    elide: Text.ElideRight
                    Layout.maximumWidth: 360
                }
            }
            Repeater {
                model: modelData.findings

                UqLabel {
                    required property var modelData
                    text: modelData.path + "  —  " + modelData.threat
                    elide: Text.ElideMiddle
                    opacity: 0.7
                    Layout.fillWidth: true
                    Layout.leftMargin: 12
                }
            }
        }
    }
    ErrorCard {
        objectName: "av-problem"
        title: "oysterAV couldn't do that"
        detail: section.host ? section.host.avProblem : ""
        visible: detail !== ""
        Layout.fillWidth: true
    }
}
