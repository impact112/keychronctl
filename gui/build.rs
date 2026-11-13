use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("io.keychron.launcher").qml_files([
        "qml/main.qml",
        "qml/DeviceView.qml",
        "qml/PerformanceView.qml",
        "qml/ButtonsView.qml",
        "qml/ProfileView.qml",
        "qml/LightingView.qml",
        "qml/RawView.qml",
        "qml/components/SectionHeader.qml",
        "qml/components/LabeledSwitch.qml",
    ]))
    .files(["src/backend.rs"])
    .build();
}
