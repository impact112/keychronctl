mod backend;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Note: do NOT install a custom objectCreationFailed handler — Qt's
    // default prints "QQmlApplicationEngine failed to load component ..."
    // to stderr, which is our only loud failure signal.
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/io/keychron/launcher/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
