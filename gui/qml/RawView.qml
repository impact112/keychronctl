import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Raw: run any backend command, inspect the JSON.
ColumnLayout {
    id: view
    spacing: 8

    property string output: ""

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        ComboBox {
            id: cmdBox
            editable: true
            model: ["list", "info", "battery", "dpi", "pollrate", "sleep", "debounce",
                    "sensor", "scroll", "settings", "lighting",
                    "profile list", "binds list", "definitions list"]
            Layout.preferredWidth: 220

            onActivated: runBtn.clicked()
        }

        Button {
            id: runBtn
            text: qsTr("Run")
            onClicked: {
                view.output = qsTr("…");
                const parts = cmdBox.editText.trim().split(/\s+/);
                const command = parts[0];
                const rest = parts.slice(1);
                const args = command === "list" ? rest : root.deviceArg().concat(rest);
                root.request(command, args, (data, code, err) => {
                    if (data !== null) view.output = JSON.stringify(data, null, 2);
                    else view.output = "error (exit " + code + "): " + err;
                });
            }
        }
    }

    ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true

        TextArea {
            readOnly: true
            font.family: "monospace"
            font.pixelSize: 12
            text: view.output
            wrapMode: TextArea.Wrap
        }
    }
}
