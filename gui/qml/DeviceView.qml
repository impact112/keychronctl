import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "components"

// Device info view: populates from `info`, `battery`, `settings`.
ScrollView {
    id: view
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    property var info: null
    property var settingsData: null

    function load() {
        if (!root.device) { view.info = null; view.settingsData = null; return; }
        root.request("info", root.deviceArg(), (d, code, err) => {
            view.info = d;
            if (d === null) root.backendError = err;
        });
        root.request("settings", root.deviceArg(), (d, code, err) => {
            view.settingsData = d;
        });
    }

    Component.onCompleted: load()
    onVisibleChanged: if (visible) load()

    ColumnLayout {
        width: view.width
        spacing: 4

        SectionHeader { title: qsTr("Device") }

        GridLayout {
            columns: 2
            columnSpacing: 24
            rowSpacing: 6

            Label { text: qsTr("Product") }
            Label { text: view.info ? view.info.product : "—"; font.bold: true }

            Label { text: qsTr("Interface") }
            Label { text: view.info ? (view.info.role + " · " + view.info.path) : "—" }

            Label { text: qsTr("VID:PID") }
            Label { text: view.info ? view.info["vid:pid"] : "—" }

            Label { text: qsTr("Protocol") }
            Label { text: view.info ? view.info.protocol : "—" }

            Label { text: qsTr("Firmware") }
            Label { text: view.info ? view.info.version : "—" }

            Label { text: qsTr("Connection") }
            Label { text: view.info ? view.info.work_mode : "—" }
        }

        SectionHeader { title: qsTr("Battery") }

        RowLayout {
            spacing: 12

            // Battery gauge (plain rectangle: Breeze's ProgressBar has
            // sizing TypeErrors with implicit layouts).
            Rectangle {
                id: batteryBar
                property int pct: view.info ? view.info.battery_percent : 0
                Layout.preferredWidth: 220
                Layout.preferredHeight: 18
                radius: 4
                border.color: palette.mid
                color: "transparent"

                Rectangle {
                    width: parent.width * parent.pct / 100
                    height: parent.height
                    radius: 4
                    color: parent.pct > 40 ? "#6aa84f" : (parent.pct > 15 ? "#cc8800" : "#cc2200")
                }

                Label {
                    anchors.centerIn: parent
                    text: parent.pct + "%"
                }
            }

            Label {
                text: view.info && view.info.charging ? qsTr("charging") : qsTr("discharging")
            }
        }

        SectionHeader { title: qsTr("Paired devices") }

        Repeater {
            model: view.info && view.info.bonded ? view.info.bonded : []
            delegate: Label {
                required property var modelData
                Layout.fillWidth: true
                text: (modelData.active ? "● " : "○ ") + (modelData.vpid) +
                      "  (" + modelData.slot + ")"
            }
        }

        Item { Layout.fillHeight: true }
    }
}
