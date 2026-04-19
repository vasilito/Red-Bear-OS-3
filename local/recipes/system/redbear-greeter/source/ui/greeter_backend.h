#pragma once

#include <QObject>
#include <QUrl>

class GreeterBackend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QUrl backgroundUrl READ backgroundUrl NOTIFY greetingChanged)
    Q_PROPERTY(QUrl iconUrl READ iconUrl NOTIFY greetingChanged)
    Q_PROPERTY(QString sessionName READ sessionName NOTIFY greetingChanged)
    Q_PROPERTY(QString state READ state NOTIFY statusChanged)
    Q_PROPERTY(QString message READ message NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    explicit GreeterBackend(QObject *parent = nullptr);

    [[nodiscard]] QUrl backgroundUrl() const;
    [[nodiscard]] QUrl iconUrl() const;
    [[nodiscard]] QString sessionName() const;
    [[nodiscard]] QString state() const;
    [[nodiscard]] QString message() const;
    [[nodiscard]] bool busy() const;

    Q_INVOKABLE void initialize();
    Q_INVOKABLE void submitLogin(const QString &username, const QString &password);
    Q_INVOKABLE void requestShutdown();
    Q_INVOKABLE void requestReboot();

signals:
    void greetingChanged();
    void statusChanged();
    void busyChanged();

private:
    struct Response {
        bool transportOk = false;
        QString transportError;
        QString type;
        bool ok = false;
        QString state;
        QString message;
        QString sessionName;
        QString backgroundPath;
        QString iconPath;
    };

    [[nodiscard]] Response sendRequest(const QByteArray &payload) const;
    void setGreeting(const QString &backgroundPath, const QString &iconPath, const QString &sessionName);
    void setStatus(const QString &state, const QString &message);
    void setBusy(bool busy);
    void applyError(const QString &message);

    QUrl m_backgroundUrl;
    QUrl m_iconUrl;
    QString m_sessionName = QStringLiteral("KDE on Wayland");
    QString m_state = QStringLiteral("starting");
    QString m_message = QStringLiteral("Connecting to greeter");
    bool m_busy = false;
};
