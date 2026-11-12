import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

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

            Label { text: qsTr("Product"); color: "#565f89" }
            Label { text: view.info ? view.info.product : "—"; color: "#c0caf5"; font.bold: true }

            Label { text: qsTr("Interface"); color: "#565f89" }
            Label { text: view.info ? (view.info.role + " · " + view.info.path) : "—"; color: "#c0caf5" }

            Label { text: qsTr("VID:PID"); color: "#565f89" }
            Label { text: view.info ? view.info["vid:pid"] : "—"; color: "#c0caf5" }

            Label { text: qsTr("Protocol"); color: "#565f89" }
            Label { text: view.info ? view.info.protocol : "—"; color: "#c0caf5" }

            Label { text: qsTr("Firmware"); color: "#565f89" }
            Label { text: view.info ? view.info.version : "—"; color: "#c0caf5" }

            Label { text: qsTr("Connection"); color: "#565f89" }
            Label { text: view.info ? view.info.work_mode : "—"; color: "#c0caf5" }
        }

        SectionHeader { title: qsTr("Battery") }

        RowLayout {
            spacing: 12

            Rectangle {
                width: 220
                height: 22
                radius: 6
                color: "#1f2033"
                border.color: "#565f89"

                Rectangle {
                    // Battery percentage bar.
                    property int pct: view.info ? view.info.battery_percent : 0
                    width: parent.width * pct / 100
                    height: parent.height
                    radius: 6
                    color: pct > 40 ? "#9ece6a" : (pct > 15 ? "#e0af68" : "#f7768e")
                }

                Label {
                    anchors.centerIn: parent
                    text: view.info ? (view.info.battery_percent + "%") : "—"
                    color: "#c0caf5"
                }
            }

            Label {
                text: view.info && view.info.charging ? qsTr("charging") : qsTr("discharging")
                color: view.info && view.info.charging ? "#9ece6a" : "#a9b1d6"
            }
        }

        SectionHeader { title: qsTr("Paired devices") }

        Repeater {
            model: view.info && view.info.bonded ? view.info.bonded : []
            delegate: Label {
                required property var modelData
                Layout.fillWidth: true
                color: modelData.active ? "#9ece6a" : "#565f89"
                text: (modelData.active ? "● " : "○ ") + (modelData.vpid) +
                      "  (" + modelData.slot + ")"
            }
        }

        Item { Layout.fillHeight: true }
    }
}
