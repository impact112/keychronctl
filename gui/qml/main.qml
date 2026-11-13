import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import io.keychronctl.qt

ApplicationWindow {
    id: root
    width: 1100
    height: 720
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: qsTr("Keychron Launcher")

    // Backend instance (drives the external CLI subprocess).
    Backend {
        id: backend
    }

    // ------- state -------
    property var device: null          // selected {vpid, path, role, product, ...}
    property var interfaces: []        // from `list`
    property string backendError: ""

    function request(command, args, cb) {
        const rid = backend.new_request_id();
        const handler = (id, ok, json, stderrText, exitCode) => {
            backend.command_finished.disconnect(handler);
            let parsed = null;
            if (ok) {
                try { parsed = JSON.parse(json); } catch (e) { parsed = null; }
                if (parsed === null) cb(null, exitCode, "unparseable JSON from backend");
                else cb(parsed, 0, "");
            } else {
                const msg = String(stderrText).split("\n").find(l => l.startsWith("error: ")) || String(stderrText);
                cb(null, exitCode, msg.replace(/^error: /, ""));
            }
        };
        backend.command_finished.connect(handler);
        backend.run(command, args, rid);
    }

    function refreshDevices() {
        request("list", [], (devices, code, err) => {
            if (devices === null) {
                root.backendError = err;
                if (code === 3) { root.interfaces = []; root.device = null; }
                return;
            }
            root.backendError = "";
            const interesting = devices.filter(d => ["mouse-8k", "mouse-4k", "keyboard"].includes(d.role));
            const changed = JSON.stringify(interesting) !== JSON.stringify(root.interfaces);
            root.interfaces = interesting;
            if (interesting.length === 0) {
                root.device = null;
                return;
            }
            // Keep the current selection if still present; else first device.
            const still = interesting.find(d => root.device && d.path === root.device.path && d.role === root.device.role);
            if (still) {
                if (changed) root.device = still;
            } else {
                root.device = interesting[0];
            }
        });
    }

    function deviceArg() {
        return root.device ? ["-d", root.device.path] : [];
    }

    Component.onCompleted: {
        backend.refresh_binary_path();
        refreshDevices();
        pollTimer.start();
    }

    // ------- layout -------
    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Sidebar
        Rectangle {
            Layout.preferredWidth: 230
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 8

                Label {
                    text: qsTr("Keychron")
                    font.pixelSize: 20
                    font.bold: true
                    Layout.bottomMargin: 4
                }

                Repeater {
                    model: root.interfaces
                    delegate: DeviceButton {
                        required property var modelData
                        dev: modelData
                        selected: root.device && modelData.path === root.device.path && modelData.role === root.device.role
                        onClicked: root.device = modelData
                        Layout.fillWidth: true
                    }
                }

                Label {
                    visible: root.interfaces.length === 0
                    text: root.backendError ? root.backendError : qsTr("No devices connected")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Item { Layout.fillHeight: true }

                Label {
                    text: backend.binary_path ? backend.binary_path : qsTr("backend not found")
                    font.pixelSize: 11
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
            }
        }

        // Content
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            TabBar {
                id: nav
                Layout.fillWidth: true

                TabButton { text: qsTr("Device") }
                TabButton { text: qsTr("Performance") }
                TabButton { text: qsTr("Buttons") }
                TabButton { text: qsTr("Profiles") }
                TabButton { text: qsTr("Lighting") }
                TabButton { text: qsTr("Raw") }
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: nav.currentIndex

                DeviceView {}
                PerformanceView {}
                ButtonsView {}
                ProfileView {}
                LightingView {}
                RawView {}
            }

            // Status bar
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 28
                visible: backend.busy || statusText.text !== ""

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    spacing: 8

                    BusyIndicator {
                        running: backend.busy
                        implicitWidth: 18
                        implicitHeight: 18
                    }
                    Label {
                        id: statusText
                        text: backend.busy ? qsTr("Working…") : root.backendError
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }

    // Device sidebar entry
    component DeviceButton: AbstractButton {
        id: btn
        property var dev
        property bool selected: false
        implicitHeight: 44

        background: Rectangle {
            radius: 6
            color: btn.selected ? palette.highlight : (btn.hovered ? palette.mid : "transparent")
        }

        contentItem: ColumnLayout {
            spacing: 0
            Label {
                text: btn.dev ? btn.dev.product : ""
                font.bold: btn.selected
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            Label {
                text: btn.dev ? (btn.dev.role + " · " + btn.dev.path) : ""
                font.pixelSize: 11
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
        }
    }

    // Poll connected devices every 3 s.
    Timer {
        id: pollTimer
        interval: 3000
        repeat: true
        onTriggered: root.refreshDevices()
    }
}
