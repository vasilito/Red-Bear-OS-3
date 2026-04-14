/*
 *  SPDX-FileCopyrightText: 2018 Aleix Pol Gonzalez <aleixpol@kde.org>
 *
 *  SPDX-License-Identifier: LGPL-2.0-or-later
 */

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {

    Component {
        id: delegateComponent
        Kirigami.Card {
            contentItem: Label { text: ourlist.prefix + index }
        }
    }

    pageStack.initialPage: Kirigami.Page {
        actions: [
            Kirigami.Action {
                text: "Switch Icon"
                onTriggered: {
                    if (icon.source === "home") {
                        icon.source = "window-new";
                    } else {
                        icon.source = "home";
                    }
                }
            },
            Kirigami.Action {
                text: "Enabled"
                checkable: true
                checked: icon.enabled
                onTriggered: icon.enabled = !icon.enabled
            },
            Kirigami.Action {
                text: "Animated"
                checkable: true
                checked: icon.animated
                onTriggered: icon.animated = !icon.animated
            },
            Kirigami.Action {
                displayComponent: RowLayout {
                    Label {
                        text: "Size:"
                    }
                    SpinBox {
                        from: 0
                        to: Kirigami.Units.iconSizes.enormous
                        value: Kirigami.Units.iconSizes.large
                        onValueModified: {
                            icon.width = value;
                            icon.height = value;
                        }
                    }
                }
            }
        ]

        Kirigami.Icon {
            id: icon
            width: Kirigami.Units.iconSizes.Large
            height: width
            source: "home"
        }
    }
}
