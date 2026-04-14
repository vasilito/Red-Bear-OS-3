/*
    This file is part of the KDE libraries
    SPDX-FileCopyrightText: 2000 Stephan Kulow <coolo@kde.org>
    SPDX-FileCopyrightText: 2000 David Faure <coolo@kde.org>
    SPDX-FileCopyrightText: 2007 Thiago Macieira <thiago@kde.org>
    SPDX-FileCopyrightText: 2024 Harald Sitter <sitter@kde.org>

    SPDX-License-Identifier: LGPL-2.0-or-later
*/

#include "connectionbackend_p.h"

#include <KLocalizedString>

using namespace KIO;

ConnectionBackend::ConnectionBackend(QObject *parent)
    : QObject(parent)
    , state(Idle)
    , socket(nullptr)
    , localServer(nullptr)
    , signalEmitted(false)
{
}

ConnectionBackend::~ConnectionBackend() = default;

void ConnectionBackend::setSuspended(bool enable)
{
    (void)enable;
}

bool ConnectionBackend::connectToRemote(const QUrl &url)
{
    (void)url;
    errorString = i18n("Local IPC is unavailable on Redox without QtNetwork");
    state = Idle;
    return false;
}

ConnectionBackend::ConnectionResult ConnectionBackend::listenForRemote()
{
    state = Idle;
    errorString = i18n("Local IPC is unavailable on Redox without QtNetwork");
    return {false, errorString};
}

bool ConnectionBackend::waitForIncomingTask(int ms)
{
    (void)ms;
    return false;
}

bool ConnectionBackend::sendCommand(int cmd, const QByteArray &data) const
{
    (void)cmd;
    (void)data;
    return false;
}

ConnectionBackend *ConnectionBackend::nextPendingConnection()
{
    return nullptr;
}

void ConnectionBackend::socketReadyRead()
{
}

void ConnectionBackend::socketDisconnected()
{
    state = Idle;
    Q_EMIT disconnected();
}
