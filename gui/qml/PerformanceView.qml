import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "components"

// Performance: DPI ladder, polling rate, sensor, debounce, sleep, scroll.
ScrollView {
    id: view
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    property var settingsData: null
    property var dpiData: null
    property var pollData: null
    property var sleepData: null
    property var debounceData: null
    property var scrollData: null

    function load() {
        if (!root.device) return;
        const args = root.deviceArg();
        root.request("settings", args, (d) => view.settingsData = d);
        root.request("dpi", args, (d) => view.dpiData = d);
        root.request("pollrate", args, (d) => view.pollData = d);
        root.request("sleep", args, (d) => view.sleepData = d);
        root.request("debounce", args, (d) => view.debounceData = d);
        root.request("scroll", args, (d) => view.scrollData = d);
    }

    function setDpi() {
        const vals = [];
        for (let i = 0; i < 5; i++) vals.push(stageRepeater.itemAt(i).value);
        const args = root.deviceArg().concat(["set", "--levels", vals.join(",")]);
        root.request("dpi", args, (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setPollrate(hz) {
        root.request("pollrate", root.deviceArg().concat(["set", String(hz)]), (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setDpiLevel(idx) {
        root.request("dpi", root.deviceArg().concat(["set", "--stage", String(idx)]), (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setSensor() {
        const s = view.settingsData ? view.settingsData.sys : null;
        if (!s) return;
        const args = root.deviceArg().concat(["set",
            "--lod", String(s.lod),
            "--wave", s.wave ? "on" : "off",
            "--line", s.line ? "on" : "off",
            "--motion", s.motion ? "on" : "off",
            "--scroll-dir", s.scroll_dir ? "reverse" : "normal",
            "--fps20k", s.fps20k ? "on" : "off"]);
        root.request("sensor", args, (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setSleep(min) {
        root.request("sleep", root.deviceArg().concat(["set", String(min)]), (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setDebounce(ms) {
        root.request("debounce", root.deviceArg().concat(["set", String(ms)]), (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    function setScroll(speed) {
        root.request("scroll", root.deviceArg().concat(["set", String(speed)]), (d, code, err) => {
            if (err) root.backendError = err; else load();
        });
    }

    Component.onCompleted: load()
    onVisibleChanged: if (visible) load()

    ColumnLayout {
        width: view.width
        spacing: 4

        // ---------------- DPI ----------------
        SectionHeader { title: qsTr("DPI stages") }

        RowLayout {
            id: stageRow
            spacing: 10

            Repeater {
                id: stageRepeater
                model: view.dpiData && view.dpiData.levels ? view.dpiData.levels : [0,0,0,0,0]

                delegate: ColumnLayout {
                    id: stage
                    required property var modelData
                    required property int index
                    property int value: modelData
                    spacing: 2

                    property bool isCurrent: view.dpiData && view.dpiData.current_levels
                                            ? view.dpiData.current_levels[0] === stage.index
                                            : false

                    Frame {
                        Layout.preferredWidth: 100
                        padding: 4

                        background: Rectangle {
                            radius: 6
                            color: stage.isCurrent ? palette.highlight : "transparent"
                            border.color: stage.isCurrent ? palette.highlight : palette.mid
                        }

                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 0
                            Label {
                                text: qsTr("Stage %1").arg(stage.index + 1)
                                font.pixelSize: 10
                                Layout.alignment: Qt.AlignHCenter
                            }
                            SpinBox {
                                from: 50
                                to: 26000
                                value: stage.value
                                editable: true
                                Layout.alignment: Qt.AlignHCenter
                                onValueModified: stage.value = value
                                font.pixelSize: 13
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: view.setDpiLevel(stage.index)
                            cursorShape: Qt.PointingHandCursor
                        }
                    }
                }
            }

            Button {
                text: qsTr("Apply stages")
                onClicked: view.setDpi()
            }
        }

        Label {
            text: qsTr("Click a stage to make it active; edit values and press Apply.")
            font.pixelSize: 11
        }

        // ---------------- Polling rate ----------------
        SectionHeader { title: qsTr("Polling rate") }

        RowLayout {
            spacing: 8
            id: rateRow

            Repeater {
                model: {
                    if (!view.pollData || !view.pollData.levelsHz) return [];
                    const seen = [], out = [];
                    for (const hz of view.pollData.levelsHz)
                        if (!seen.includes(hz)) { seen.push(hz); out.push(hz); }
                    return out;
                }

                delegate: Button {
                    required property var modelData
                    checkable: true
                    checked: view.pollData && view.pollData.hz === modelData
                    text: modelData + " Hz"
                    onClicked: view.setPollrate(modelData)
                }
            }
        }

        // ---------------- Sensor ----------------
        SectionHeader { title: qsTr("Sensor") }

        GridLayout {
            columns: 2
            columnSpacing: 24
            rowSpacing: 2

            LabeledSwitch {
                label: qsTr("Motion sync")
                checked: view.settingsData && view.settingsData.sys.motion
                onToggled: (chk) => { view.settingsData.sys.motion = chk; view.setSensor(); }
            }
            LabeledSwitch {
                label: qsTr("Angle snapping")
                checked: view.settingsData && view.settingsData.sys.line
                onToggled: (chk) => { view.settingsData.sys.line = chk; view.setSensor(); }
            }
            LabeledSwitch {
                label: qsTr("Ripple control")
                checked: view.settingsData && view.settingsData.sys.wave
                onToggled: (chk) => { view.settingsData.sys.wave = chk; view.setSensor(); }
            }
            LabeledSwitch {
                label: qsTr("Reverse scroll direction")
                checked: view.settingsData && view.settingsData.sys.scroll_dir
                onToggled: (chk) => { view.settingsData.sys.scroll_dir = chk; view.setSensor(); }
            }
            LabeledSwitch {
                label: qsTr("20K max-speed mode")
                checked: view.settingsData && view.settingsData.sys.fps20k
                onToggled: (chk) => { view.settingsData.sys.fps20k = chk; view.setSensor(); }
            }

            RowLayout {
                Label { text: qsTr("Lift-off distance") }
                ComboBox {
                    model: ["0", "1", "2", "3"]
                    currentIndex: view.settingsData ? view.settingsData.sys.lod : 0
                    onActivated: (i) => { view.settingsData.sys.lod = i; view.setSensor(); }
                }
            }
        }

        // ---------------- Debounce ----------------
        SectionHeader { title: qsTr("Debounce") }

        RowLayout {
            Label { text: qsTr("Response time") }
            ComboBox {
                model: {
                    const opts = view.debounceData && view.debounceData.options
                        ? view.debounceData.options : [];
                    const seen = [], out = [];
                    for (const o of opts) if (!seen.includes(o)) { seen.push(o); out.push(o + " ms"); }
                    return out.length ? out : ["4 ms"];
                }
                onActivated: (i) => {
                    const opts = view.debounceData.options;
                    const seen = [], uniq = [];
                    for (const o of opts) if (!seen.includes(o)) { seen.push(o); uniq.push(o); }
                    view.setDebounce(uniq[i]);
                }
            }
        }

        // ---------------- Sleep ----------------
        SectionHeader { title: qsTr("Sleep timer") }

        RowLayout {
            Label { text: qsTr("Device sleeps after") }
            ComboBox {
                id: sleepCombo
                model: [qsTr("1 min"), qsTr("5 min"), qsTr("10 min"), qsTr("15 min"),
                        qsTr("30 min"), qsTr("60 min"), qsTr("Never")]
                property var values: [1, 5, 10, 15, 30, 60, 0]
                currentIndex: {
                    const m = view.sleepData ? view.sleepData.minutes : 10;
                    const idx = values.indexOf(m);
                    return idx >= 0 ? idx : 2;
                }
                onActivated: (i) => view.setSleep(values[i])
            }
        }

        // ---------------- Scroll ----------------
        SectionHeader { title: qsTr("Scroll wheel") }

        RowLayout {
            Label { text: qsTr("Speed") }
            SpinBox {
                from: 0
                to: 100
                value: view.scrollData ? view.scrollData.speed : 0
                onValueModified: view.setScroll(value)
            }
        }

        Item { Layout.fillHeight: true }
    }
}
