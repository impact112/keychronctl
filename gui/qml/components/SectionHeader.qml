import QtQuick
import QtQuick.Controls

// Shared section header.
Label {
    property string title: ""
    text: title
    font.pixelSize: 15
    font.bold: true
    topPadding: 16
    bottomPadding: 6
}
