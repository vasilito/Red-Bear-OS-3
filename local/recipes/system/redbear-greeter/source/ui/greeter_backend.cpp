#include "greeter_backend.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>

#include <poll.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cerrno>
#include <cstddef>
#include <cstring>

namespace {
constexpr auto kGreeterSocketPath = "/run/redbear-greeterd.sock";
constexpr auto kConnectTimeoutMs = 1500;
constexpr auto kReadTimeoutMs = 5000;

bool waitForReadable(int fd, int timeoutMs, QString *error) {
    pollfd descriptor{};
    descriptor.fd = fd;
    descriptor.events = POLLIN;

    const auto pollResult = ::poll(&descriptor, 1, timeoutMs);
    if (pollResult > 0) {
        return true;
    }
    if (pollResult == 0) {
        *error = QStringLiteral("timed out waiting for greeter response");
        return false;
    }

    *error = QStringLiteral("failed while waiting for greeter response: %1").arg(QString::fromLocal8Bit(std::strerror(errno)));
    return false;
}
}

GreeterBackend::GreeterBackend(QObject *parent) : QObject(parent) {}

QUrl GreeterBackend::backgroundUrl() const {
    return m_backgroundUrl;
}

QUrl GreeterBackend::iconUrl() const {
    return m_iconUrl;
}

QString GreeterBackend::sessionName() const {
    return m_sessionName;
}

QString GreeterBackend::state() const {
    return m_state;
}

QString GreeterBackend::message() const {
    return m_message;
}

bool GreeterBackend::busy() const {
    return m_busy;
}

void GreeterBackend::initialize() {
    const auto response = sendRequest(QJsonDocument(QJsonObject{{QStringLiteral("type"), QStringLiteral("hello")},
                                                                {QStringLiteral("version"), 1}})
                                          .toJson(QJsonDocument::Compact));
    if (!response.transportOk) {
        applyError(response.transportError);
        return;
    }

    if (response.type != QStringLiteral("hello_ok")) {
        applyError(response.message.isEmpty() ? QStringLiteral("unexpected greeter hello response") : response.message);
        return;
    }

    setGreeting(response.backgroundPath, response.iconPath, response.sessionName);
    setStatus(response.state, response.message);
}

void GreeterBackend::submitLogin(const QString &username, const QString &password) {
    if (m_busy) {
        return;
    }
    if (username.trimmed().isEmpty() || password.isEmpty()) {
        setStatus(QStringLiteral("greeter_ready"), QStringLiteral("Enter both username and password."));
        return;
    }

    setBusy(true);
    setStatus(QStringLiteral("authenticating"), QStringLiteral("Authenticating"));

    const auto response = sendRequest(QJsonDocument(QJsonObject{{QStringLiteral("type"), QStringLiteral("submit_login")},
                                                                {QStringLiteral("username"), username},
                                                                {QStringLiteral("password"), password}})
                                          .toJson(QJsonDocument::Compact));
    setBusy(false);
    if (!response.transportOk) {
        applyError(response.transportError);
        return;
    }

    if (response.type == QStringLiteral("login_result")) {
        setStatus(response.state, response.message);
        if (response.ok) {
            QTimer::singleShot(0, qApp, &QCoreApplication::quit);
        }
        return;
    }

    applyError(response.message.isEmpty() ? QStringLiteral("unexpected login response") : response.message);
}

void GreeterBackend::requestShutdown() {
    if (m_busy) {
        return;
    }

    setBusy(true);
    setStatus(QStringLiteral("power_action"), QStringLiteral("Requesting shutdown"));
    const auto response = sendRequest(
        QJsonDocument(QJsonObject{{QStringLiteral("type"), QStringLiteral("request_shutdown")}})
            .toJson(QJsonDocument::Compact));
    setBusy(false);

    if (!response.transportOk) {
        applyError(response.transportError);
        return;
    }

    if (response.type == QStringLiteral("action_result")) {
        setStatus(response.ok ? QStringLiteral("power_action") : QStringLiteral("greeter_ready"), response.message);
        return;
    }

    applyError(response.message.isEmpty() ? QStringLiteral("unexpected shutdown response") : response.message);
}

void GreeterBackend::requestReboot() {
    if (m_busy) {
        return;
    }

    setBusy(true);
    setStatus(QStringLiteral("power_action"), QStringLiteral("Requesting reboot"));
    const auto response = sendRequest(
        QJsonDocument(QJsonObject{{QStringLiteral("type"), QStringLiteral("request_reboot")}})
            .toJson(QJsonDocument::Compact));
    setBusy(false);

    if (!response.transportOk) {
        applyError(response.transportError);
        return;
    }

    if (response.type == QStringLiteral("action_result")) {
        setStatus(response.ok ? QStringLiteral("power_action") : QStringLiteral("greeter_ready"), response.message);
        return;
    }

    applyError(response.message.isEmpty() ? QStringLiteral("unexpected reboot response") : response.message);
}

GreeterBackend::Response GreeterBackend::sendRequest(const QByteArray &payload) const {
    Response response;

    const int fd = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) {
        response.transportError = QStringLiteral("failed to create greeter socket: %1")
                                      .arg(QString::fromLocal8Bit(std::strerror(errno)));
        return response;
    }

    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    std::strncpy(address.sun_path, kGreeterSocketPath, sizeof(address.sun_path) - 1);
    const auto addressSize = static_cast<socklen_t>(offsetof(sockaddr_un, sun_path) + std::strlen(address.sun_path) + 1);
    if (::connect(fd, reinterpret_cast<sockaddr *>(&address), addressSize) != 0) {
        response.transportError = QStringLiteral("failed to connect to %1: %2")
                                      .arg(QString::fromLatin1(kGreeterSocketPath),
                                           QString::fromLocal8Bit(std::strerror(errno)));
        ::close(fd);
        return response;
    }

    const auto fullPayload = payload + '\n';
    qsizetype written = 0;
    while (written < fullPayload.size()) {
        const auto chunk = ::write(fd, fullPayload.constData() + written, static_cast<size_t>(fullPayload.size() - written));
        if (chunk < 0) {
            response.transportError = QStringLiteral("failed to write greeter request: %1")
                                          .arg(QString::fromLocal8Bit(std::strerror(errno)));
            ::close(fd);
            return response;
        }
        written += chunk;
    }

    QString waitError;
    if (!waitForReadable(fd, kReadTimeoutMs, &waitError)) {
        response.transportError = waitError;
        ::close(fd);
        return response;
    }

    QByteArray reply;
    char buffer[1024];
    while (reply.indexOf('\n') < 0) {
        const auto chunk = ::read(fd, buffer, sizeof(buffer));
        if (chunk < 0) {
            response.transportError = QStringLiteral("failed to read greeter response: %1")
                                          .arg(QString::fromLocal8Bit(std::strerror(errno)));
            ::close(fd);
            return response;
        }
        if (chunk == 0) {
            break;
        }
        reply.append(buffer, static_cast<int>(chunk));
        if (reply.indexOf('\n') < 0 && !waitForReadable(fd, kConnectTimeoutMs, &waitError)) {
            response.transportError = waitError;
            ::close(fd);
            return response;
        }
    }
    ::close(fd);

    const auto newlineIndex = reply.indexOf('\n');
    if (newlineIndex >= 0) {
        reply.truncate(newlineIndex);
    }

    const auto document = QJsonDocument::fromJson(reply);
    if (!document.isObject()) {
        response.transportError = QStringLiteral("invalid greeter response payload");
        return response;
    }

    const auto object = document.object();
    response.transportOk = true;
    response.type = object.value(QStringLiteral("type")).toString();
    response.ok = object.value(QStringLiteral("ok")).toBool();
    response.state = object.value(QStringLiteral("state")).toString();
    response.message = object.value(QStringLiteral("message")).toString();
    response.sessionName = object.value(QStringLiteral("session_name")).toString();
    response.backgroundPath = object.value(QStringLiteral("background")).toString();
    response.iconPath = object.value(QStringLiteral("icon")).toString();
    if (response.type == QStringLiteral("error") && response.message.isEmpty()) {
        response.message = QStringLiteral("greeter returned an unspecified error");
    }
    return response;
}

void GreeterBackend::setGreeting(const QString &backgroundPath, const QString &iconPath, const QString &sessionName) {
    const auto nextBackground = backgroundPath.isEmpty() ? QUrl() : QUrl::fromLocalFile(backgroundPath);
    const auto nextIcon = iconPath.isEmpty() ? QUrl() : QUrl::fromLocalFile(iconPath);
    const auto nextSessionName = sessionName.isEmpty() ? QStringLiteral("KDE on Wayland") : sessionName;

    if (m_backgroundUrl == nextBackground && m_iconUrl == nextIcon && m_sessionName == nextSessionName) {
        return;
    }

    m_backgroundUrl = nextBackground;
    m_iconUrl = nextIcon;
    m_sessionName = nextSessionName;
    emit greetingChanged();
}

void GreeterBackend::setStatus(const QString &state, const QString &message) {
    const auto nextState = state.isEmpty() ? QStringLiteral("greeter_ready") : state;
    if (m_state == nextState && m_message == message) {
        return;
    }

    m_state = nextState;
    m_message = message;
    emit statusChanged();
}

void GreeterBackend::setBusy(bool busy) {
    if (m_busy == busy) {
        return;
    }

    m_busy = busy;
    emit busyChanged();
}

void GreeterBackend::applyError(const QString &message) {
    setStatus(QStringLiteral("fatal_error"), message);
}
