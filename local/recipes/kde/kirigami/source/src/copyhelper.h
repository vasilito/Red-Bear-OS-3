/*
 *  SPDX-FileCopyrightText: 2009 Alan Alpert <alan.alpert@nokia.com>
 *  SPDX-FileCopyrightText: 2010 Ménard Alexis <menard@kde.org>
 *  SPDX-FileCopyrightText: 2010 Marco Martin <mart@kde.org>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

#ifndef COPYHELPER_H
#define COPYHELPER_H

#include <QObject>
#include <qqmlregistration.h>

class CopyHelperPrivate : public QObject
{
    Q_OBJECT
    QML_ELEMENT_OFF_OFF_OFF_OFF_OFF_OFF
    QML_SINGLETON_OFF_OFF_OFF_OFF_OFF_OFF
public:
    Q_INVOKABLE void copyTextToClipboard(const QString &text);
};

#endif
