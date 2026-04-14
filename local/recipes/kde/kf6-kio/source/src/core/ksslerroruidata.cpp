/*
    This file is part of the KDE libraries
    SPDX-FileCopyrightText: 2007, 2008 Andreas Hartmetz <ahartmetz@gmail.com>

    SPDX-License-Identifier: LGPL-2.0-or-later
*/

#include "ksslerroruidata.h"
#include "ksslerroruidata_p.h"

KSslErrorUiData::KSslErrorUiData()
    : d(new Private())
{
    d->usedBits = 0;
    d->bits = 0;
}

KSslErrorUiData::KSslErrorUiData(const QSslSocket *socket)
    : d(new Private())
{
    d->usedBits = 0;
    d->bits = 0;
    (void)socket;
}

KSslErrorUiData::KSslErrorUiData(const QNetworkReply *reply, const QList<QSslError> &sslErrors)
    : d(new Private())
{
    d->usedBits = 0;
    d->bits = 0;
    (void)reply;
    (void)sslErrors;
}

KSslErrorUiData::KSslErrorUiData(const KSslErrorUiData &other)
    : d(new Private(*other.d))
{
}

KSslErrorUiData::~KSslErrorUiData() = default;

KSslErrorUiData &KSslErrorUiData::operator=(const KSslErrorUiData &other)
{
    *d = *other.d;
    return *this;
}
