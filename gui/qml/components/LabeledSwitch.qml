import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Switch with a label on the left.
RowLayout {
    id: control
    property string label: ""
    property bool checked: false
    signal toggled(bool checked)

    spacing: 8
    Label {
        text: control.label
        color: "#a9b1d6"
        Layout.fillWidth: true
    }
    Switch {
        checked: control.checked
        onToggled: control.toggled(checked)
    }
}
