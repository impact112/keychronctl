import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "components"

// Lighting: mode / brightness / speed / color. Devices without RGB simply
// fail the query; the view shows a hint instead.
ScrollView {
    id: view
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    property var lightData: null
    property bool noRgb: false

    function load() {
        if (!root.device) return;
        root.request("lighting", root.deviceArg(), (d, code, err) => {
            view.lightData = d;
            view.noRgb = (d === null);
        });
    }

    function apply() {
        if (!view.lightData) return;
        const l = view.lightData;
        const args = root.deviceArg().concat([
            "set", String(l.mode),
            "--brightness", String(l.brightness),
            "--speed", String(l.speed),
            "--rgb", [l.rgb[0], l.rgb[1], l.rgb[2]].join(",")
        ]);
        root.request("lighting", args, (d, code, err) => {
            if (err) { root.backendError = err; view.noRgb = true; }
            else { view.lightData = d; view.noRgb = false; }
        });
    }

    Component.onCompleted: load()
    onVisibleChanged: if (visible) load()

    ColumnLayout {
        width: view.width
        spacing: 4

        SectionHeader { title: qsTr("Lighting") }

        Label {
            visible: view.noRgb
            text: qsTr("This device does not appear to have RGB lighting.")
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        ColumnLayout {
            visible: !view.noRgb && view.lightData
            spacing: 8
            Layout.fillWidth: true

            RowLayout {
                Label { text: qsTr("Mode") }
                SpinBox {
                    from: 0
                    to: 20
                    value: view.lightData ? view.lightData.mode : 0
                    onValueModified: { view.lightData.mode = value; view.apply(); }
                }
            }
            RowLayout {
                Label { text: qsTr("Brightness") }
                Slider {
                    from: 0
                    to: 100
                    value: view.lightData ? view.lightData.brightness : 50
                    Layout.preferredWidth: 200
                    onMoved: { view.lightData.brightness = value; }
                }
            }
            RowLayout {
                Label { text: qsTr("Speed") }
                Slider {
                    from: 0
                    to: 100
                    value: view.lightData ? view.lightData.speed : 50
                    Layout.preferredWidth: 200
                    onMoved: { view.lightData.speed = value; }
                }
            }
            RowLayout {
                Label { text: qsTr("Color") }
                Button {
                    // Color swatch preview + picker trigger.
                    property var rgb: view.lightData ? view.lightData.rgb : [128,128,128]
                    background: Rectangle {
                        implicitWidth: 40
                        implicitHeight: 24
                        radius: 6
                        color: Qt.rgba(parent.rgb[0] / 255, parent.rgb[1] / 255, parent.rgb[2] / 255, 1)
                        border.color: palette.mid
                    }
                    text: ""
                    onClicked: colorDialog.open()
                }
            }
            Button {
                text: qsTr("Apply")
                onClicked: view.apply()
            }
        }

        Item { Layout.fillHeight: true }
    }

    Popup {
        id: colorDialog
        anchors.centerIn: parent
        width: 260
        height: 320
        modal: true
        padding: 12

        contentItem: ColumnLayout {
            spacing: 10
            Label { text: qsTr("Pick a color") }

            Slider {
                id: sr
                Layout.fillWidth: true
                from: 0; to: 255
                value: view.lightData ? view.lightData.rgb[0] : 255
                Text { text: "R " + parent.value; anchors.left: parent.right; anchors.leftMargin: 8 }
            }
            Slider {
                id: sg
                Layout.fillWidth: true
                from: 0; to: 255
                value: view.lightData ? view.lightData.rgb[1] : 255
                Text { text: "G " + parent.value; anchors.left: parent.right; anchors.leftMargin: 8 }
            }
            Slider {
                id: sb
                Layout.fillWidth: true
                from: 0; to: 255
                value: view.lightData ? view.lightData.rgb[2] : 255
                Text { text: "B " + parent.value; anchors.left: parent.right; anchors.leftMargin: 8 }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 30
                radius: 6
                color: Qt.rgba(sr.value / 255, sg.value / 255, sb.value / 255, 1)
            }

            Button {
                Layout.fillWidth: true
                text: qsTr("OK")
                onClicked: {
                    if (view.lightData) {
                        view.lightData.rgb = [sr.value, sg.value, sb.value];
                        view.apply();
                    }
                    colorDialog.close();
                }
            }
        }
    }
}
