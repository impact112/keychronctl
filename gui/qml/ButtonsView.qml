import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Buttons: per-profile button bindings with an action picker.
ScrollView {
    id: view
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    property var bindsData: null
    property int currentProfile: 0

    function load() {
        if (!root.device) { view.bindsData = null; return; }
        root.request("binds", root.deviceArg().concat(["list"]), (d, code, err) => {
            view.bindsData = d;
            if (d === null) root.backendError = err;
        });
    }

    function setBind(index, action) {
        const args = root.deviceArg().concat(["set", String(index), action]);
        root.request("binds", args, (d, code, err) => {
            if (err) root.backendError = err;
            load();
        });
    }

    Component.onCompleted: load()
    onVisibleChanged: if (visible) load()

    ColumnLayout {
        width: view.width
        spacing: 4

        SectionHeader { title: qsTr("Button bindings") }

        Label {
            visible: !view.bindsData
            text: qsTr("No binding data — is the device connected?")
            color: "#565f89"
        }

        // Profile tabs
        TabBar {
            id: profileTabs
            visible: view.bindsData && view.bindsData.profiles
            onCurrentIndexChanged: view.currentProfile = currentIndex

            Repeater {
                model: view.bindsData && view.bindsData.profiles ? view.bindsData.profiles.length : 0
                delegate: TabButton {
                    required property int index
                    text: qsTr("Profile %1").arg(index + 1)
                    checked: profileTabs.currentIndex === index
                }
            }
        }

        // Binding rows for the selected profile
        ListView {
            id: bindsList
            visible: view.bindsData && view.bindsData.profiles
            Layout.fillWidth: true
            Layout.preferredHeight: contentItem.implicitHeight
            interactive: false
            spacing: 2

            model: view.bindsData && view.bindsData.profiles && view.bindsData.profiles.length > 0
                   ? view.bindsData.profiles[view.currentProfile].bindings : []

            delegate: Rectangle {
                id: row
                required property var modelData
                width: bindsList.width
                height: 40
                radius: 6
                color: mouseArea.containsMouse ? "#1f2033" : "transparent"

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 12

                    Label {
                        text: "[" + row.modelData.index + "]"
                        color: "#565f89"
                        font.pixelSize: 12
                    }
                    Label {
                        text: row.modelData.key || ""
                        color: "#c0caf5"
                        Layout.preferredWidth: 110
                    }
                    Label {
                        text: row.modelData.action +
                              (row.modelData.value ? (":" + row.modelData.value) : "")
                        color: "#7aa2f7"
                        font.family: "monospace"
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Change")
                        flat: true
                        onClicked: picker.openFor(row.modelData.index)
                    }
                }

                MouseArea {
                    id: mouseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: picker.openFor(row.modelData.index)
                }
            }
        }

        Label {
            visible: view.bindsData && view.bindsData.profiles
                     && view.bindsData.profiles.length > view.currentProfile
                     && view.bindsData.profiles[view.currentProfile].bindings.length === 0
            text: qsTr("No custom bindings on this profile")
            color: "#565f89"
        }

        Item { Layout.fillHeight: true }
    }

    // ---------------- Action picker ----------------
    Popup {
        id: picker
        property int bindIndex: 0
        property string currentAction: ""

        function openFor(index) {
            picker.bindIndex = index;
            picker.open();
        }

        anchors.centerIn: parent
        width: 420
        height: 460
        modal: true
        focus: true
        padding: 12

        background: Rectangle {
            radius: 12
            color: "#1f2033"
            border.color: "#565f89"
        }

        contentItem: ColumnLayout {
            spacing: 8

            Label {
                text: qsTr("Bind button %1").arg(picker.bindIndex)
                color: "#e6e9f0"
                font.bold: true
                font.pixelSize: 15
            }

            TabBar {
                id: pickerTabs
                Layout.fillWidth: true
                Repeater {
                    model: [qsTr("Mouse"), qsTr("Media"), qsTr("Keys"), qsTr("DPI"), qsTr("Other")]
                    delegate: TabButton { required property string modelData; text: modelData }
                }
            }

            StackLayout {
                currentIndex: pickerTabs.currentIndex
                Layout.fillWidth: true
                Layout.fillHeight: true

                // Mouse actions
                Flow {
                    spacing: 6
                    Repeater {
                        model: ["left", "right", "middle", "forward", "backward",
                                "upScroll", "downScroll", "leftScroll", "rightScroll"]
                        delegate: Button {
                            required property string modelData
                            text: modelData
                            onClicked: { view.setBind(picker.bindIndex, "mouse:" + modelData); picker.close(); }
                        }
                    }
                }

                // Media actions
                Flow {
                    spacing: 6
                    Repeater {
                        model: ["volumeUp", "volumeDown", "mute", "player",
                                "pause", "previous", "next", "stop"]
                        delegate: Button {
                            required property string modelData
                            text: modelData
                            onClicked: { view.setBind(picker.bindIndex, "media:" + modelData); picker.close(); }
                        }
                    }
                }

                // Keyboard combos
                ColumnLayout {
                    spacing: 8
                    Label { text: qsTr("Modifiers"); color: "#565f89" }
                    Flow {
                        id: mods
                        spacing: 6
                        property var chosen: ({})
                        Repeater {
                            model: ["ctrl", "shift", "alt", "win"]
                            delegate: Button {
                                required property string modelData
                                checkable: true
                                text: modelData
                                onCheckedChanged: mods.chosen[modelData] = checked
                            }
                        }
                    }
                    Label { text: qsTr("Key"); color: "#565f89" }
                    TextField {
                        id: keyField
                        placeholderText: qsTr("e.g. t, F5, ENTER")
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Apply")
                        Layout.fillWidth: true
                        onClicked: {
                            const parts = [];
                            for (const m of ["ctrl", "shift", "alt", "win"])
                                if (mods.chosen[m]) parts.push(m);
                            parts.push(keyField.text.trim());
                            view.setBind(picker.bindIndex, "keyboard:" + parts.join("+"));
                            picker.close();
                        }
                    }
                }

                // DPI / polling actions
                Flow {
                    spacing: 6
                    Repeater {
                        model: ["loop", "next", "prev"]
                        delegate: Button {
                            required property string modelData
                            text: "dpi:" + modelData
                            onClicked: { view.setBind(picker.bindIndex, "dpi:" + modelData); picker.close(); }
                        }
                    }
                    Repeater {
                        model: ["loop", "next", "prev"]
                        delegate: Button {
                            required property string modelData
                            text: "pollingRate:" + modelData
                            onClicked: { view.setBind(picker.bindIndex, "pollingRate:" + modelData); picker.close(); }
                        }
                    }
                }

                // Shortcuts / profile / remove / disable
                Flow {
                    spacing: 6
                    Repeater {
                        model: ["copy", "paste", "cut", "refresh", "switchApp", "task",
                                "brightnessUp", "brightnessDown", "home", "email"]
                        delegate: Button {
                            required property string modelData
                            text: modelData
                            onClicked: { view.setBind(picker.bindIndex, "shortcut:" + modelData); picker.close(); }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Button {
                    text: qsTr("Remove binding")
                    onClicked: { view.setBind(picker.bindIndex, "remove"); picker.close(); }
                }
                Button {
                    text: qsTr("Disable button")
                    onClicked: { view.setBind(picker.bindIndex, "disable"); picker.close(); }
                }
                Item { Layout.fillWidth: true }
                Button {
                    text: qsTr("Close")
                    flat: true
                    onClicked: picker.close()
                }
            }
        }
    }
}
