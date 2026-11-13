import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "components"

// Profiles: list, select, create, remove.
ScrollView {
    id: view
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    property var profileData: null

    function load() {
        if (!root.device) { view.profileData = null; return; }
        root.request("profile", root.deviceArg().concat(["list"]), (d, code, err) => {
            view.profileData = d;
            if (d === null) root.backendError = err;
        });
    }

    function op(sub) {
        root.request("profile", root.deviceArg().concat([sub]), (d, code, err) => {
            if (err) root.backendError = err;
            load();
        });
    }

    function select(idx) {
        root.request("profile", root.deviceArg().concat(["select", String(idx)]), (d, code, err) => {
            if (err) root.backendError = err;
            load();
        });
    }

    Component.onCompleted: load()
    onVisibleChanged: if (visible) load()

    ColumnLayout {
        width: view.width
        spacing: 4

        SectionHeader { title: qsTr("Profiles") }

        Label {
            visible: !view.profileData
            text: qsTr("No profile data — is the device connected?")
        }

        Repeater {
            model: view.profileData ? view.profileData.count : 0

            delegate: ItemDelegate {
                id: row
                required property int index
                property bool isCurrent: view.profileData && view.profileData.current === row.index
                Layout.fillWidth: true
                height: 44

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12

                    Label {
                        text: (row.isCurrent ? "● " : "○ ") + qsTr("Profile %1").arg(row.index + 1)
                        font.bold: row.isCurrent
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Activate")
                        enabled: !row.isCurrent
                        onClicked: view.select(row.index)
                    }
                }
            }
        }

        RowLayout {
            spacing: 8
            Layout.topMargin: 12

            Button {
                text: qsTr("Add profile")
                onClicked: view.op("create")
            }
            Button {
                text: qsTr("Remove last")
                enabled: view.profileData && view.profileData.count > 1
                onClicked: view.op("remove")
            }
        }

        Item { Layout.fillHeight: true }
    }
}
