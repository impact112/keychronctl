import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

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
            color: "#565f89"
        }

        Repeater {
            model: view.profileData ? view.profileData.count : 0

            delegate: Rectangle {
                id: row
                required property int index
                property bool isCurrent: view.profileData && view.profileData.current === row.index
                Layout.fillWidth: true
                height: 44
                radius: 8
                color: row.isCurrent ? "#2a2b3d" : "#1f2033"
                border.width: row.isCurrent ? 1 : 0
                border.color: "#7aa2f7"

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12

                    Label {
                        text: (row.isCurrent ? "● " : "○ ") + qsTr("Profile %1").arg(row.index + 1)
                        color: row.isCurrent ? "#c0caf5" : "#a9b1d6"
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
