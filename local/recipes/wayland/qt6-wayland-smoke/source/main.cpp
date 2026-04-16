#include <QByteArray>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QWindow>
#include <QBackingStore>
#include <QPainter>
#include <QDebug>
#include <QTimer>

#include <cstdio>

class ColoredWindow : public QWindow {
public:
    ColoredWindow() : QWindow(), m_backingStore(this) {
        resize(320, 240);
    }

    void exposeEvent(QExposeEvent *) override {
        if (!isExposed())
            return;
        render();
    }

    void resizeEvent(QResizeEvent *) override {
        m_backingStore.resize(size());
        if (isExposed())
            render();
    }

    void render() {
        QRect rect(0, 0, width(), height());
        m_backingStore.beginPaint(rect);
        QPaintDevice *device = m_backingStore.paintDevice();
        if (device) {
            QPainter p(device);
            p.fillRect(rect, QColor(180, 30, 30));
            p.setPen(Qt::white);
            p.drawText(rect, Qt::AlignCenter,
                QStringLiteral("Red Bear OS\nQt6 Wayland Smoke Test"));
        }
        m_backingStore.endPaint();
        m_backingStore.flush(rect);
    }

private:
    QBackingStore m_backingStore;
};

int main(int argc, char **argv) {
    const QByteArray platform = qEnvironmentVariableIsSet("QT_QPA_PLATFORM")
            ? qgetenv("QT_QPA_PLATFORM")
            : QByteArray("wayland");

    qputenv("QT_QPA_PLATFORM", platform);
    std::fprintf(stderr, "qt6-wayland-smoke before QGuiApplication platform=%s\n", platform.constData());

    QGuiApplication app(argc, argv);
    std::fprintf(stderr, "qt6-wayland-smoke after QGuiApplication platform=%s\n", platform.constData());

    qInfo() << "qt6-wayland-smoke platform" << QGuiApplication::platformName();

    ColoredWindow window;
    window.setTitle(QStringLiteral("Red Bear OS - Qt6 Wayland Smoke"));
    window.show();
    std::fprintf(stderr, "qt6-wayland-smoke window shown, %dx%d\n",
                 window.width(), window.height());

    QTimer::singleShot(3000, &app, &QCoreApplication::quit);
    return app.exec();
}
