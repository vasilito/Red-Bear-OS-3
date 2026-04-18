/*
    KWin - the KDE window manager
    This file is part of the KDE project.

    SPDX-FileCopyrightText: 2011 Lionel Chauvin <megabigbug@yahoo.fr>
    SPDX-FileCopyrightText: 2011, 2012 Cédric Bellegarde <gnumdk@gmail.com>
    SPDX-FileCopyrightText: 2013 Martin Gräßlin <mgraesslin@kde.org>

    SPDX-License-Identifier: GPL-2.0-or-later
*/
#include "appmenu.h"
#include "window.h"
#include "workspace.h"
#include <QDBusObjectPath>

#include "decorations/decorationbridge.h"
#include <KDecoration3/DecorationSettings>

namespace KWin
{

ApplicationMenu::ApplicationMenu()
{
}

bool ApplicationMenu::applicationMenuEnabled() const
{
    return m_applicationMenuEnabled;
}

void ApplicationMenu::setViewEnabled(bool enabled)
{
    Q_UNUSED(enabled)
}

void ApplicationMenu::slotShowRequest(const QString &serviceName, const QDBusObjectPath &menuObjectPath, int actionId)
{
    // Ignore show request when user has not configured the application menu title bar button
    auto decorationSettings = Workspace::self()->decorationBridge()->settings();
    if (decorationSettings && !decorationSettings->decorationButtonsLeft().contains(KDecoration3::DecorationButtonType::ApplicationMenu)
        && !decorationSettings->decorationButtonsRight().contains(KDecoration3::DecorationButtonType::ApplicationMenu)) {
        return;
    }

    if (Window *window = findWindowWithApplicationMenu(serviceName, menuObjectPath)) {
        window->showApplicationMenu(actionId);
    }
}

void ApplicationMenu::slotMenuShown(const QString &serviceName, const QDBusObjectPath &menuObjectPath)
{
    if (Window *window = findWindowWithApplicationMenu(serviceName, menuObjectPath)) {
        window->setApplicationMenuActive(true);
    }
}

void ApplicationMenu::slotMenuHidden(const QString &serviceName, const QDBusObjectPath &menuObjectPath)
{
    if (Window *window = findWindowWithApplicationMenu(serviceName, menuObjectPath)) {
        window->setApplicationMenuActive(false);
    }
}

void ApplicationMenu::showApplicationMenu(const QPoint &p, Window *c, int actionId)
{
    Q_UNUSED(p)
    Q_UNUSED(c)
    Q_UNUSED(actionId)
}

Window *ApplicationMenu::findWindowWithApplicationMenu(const QString &serviceName, const QDBusObjectPath &menuObjectPath)
{
    if (serviceName.isEmpty() || menuObjectPath.path().isEmpty()) {
        return nullptr;
    }

    return Workspace::self()->findWindow([&](const Window *window) {
        return window->applicationMenuServiceName() == serviceName
            && window->applicationMenuObjectPath() == menuObjectPath.path();
    });
}

} // namespace KWin

#include "moc_appmenu.cpp"
