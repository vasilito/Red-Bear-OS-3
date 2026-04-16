#include <QByteArray>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QDebug>
#include <QTimer>

#include <cstdio>

int main(int argc, char **argv) {
    const QByteArray platform = qEnvironmentVariableIsSet("QT_QPA_PLATFORM")
            ? qgetenv("QT_QPA_PLATFORM")
            : QByteArray("wayland");

    qputenv("QT_QPA_PLATFORM", platform);
    std::fprintf(stderr, "qt6-wayland-smoke before QGuiApplication platform=%s\n", platform.constData());

    QGuiApplication app(argc, argv);
    std::fprintf(stderr, "qt6-wayland-smoke after QGuiApplication platform=%s\n", platform.constData());

    qInfo() << "qt6-wayland-smoke platform" << QGuiApplication::platformName();

    QTimer::singleShot(1000, &app, &QCoreApplication::quit);
    return app.exec();
}
