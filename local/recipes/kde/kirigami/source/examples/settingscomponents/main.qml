/*
 *  SPDX-FileCopyrightText: 2021 Felipe Kinoshita <kinofhek@gmail.com>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {
    id: root

    title: qsTr("Hello, World")

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: !Kirigami.isMobile
        actions: [
            Kirigami.Action {
                text: qsTr("Settings")
                icon.name: "settings-configure"
                onTriggered: root.pageStack.pushDialogLayer(Qt.resolvedUrl("./SettingsPage.qml"), {
                    width: root.width
                }, {
                    title: qsTr("Settings"),
                    width: root.width - (Kirigami.Units.gridUnit * 4),
                    height: root.height - (Kirigami.Units.gridUnit * 4)
                })
            }
        ]
    }

    pageStack.initialPage: Kirigami.Page {
        title: qsTr("Main Page")
    }
}
