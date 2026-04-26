import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: root
    visible: true
    visibility: Window.FullScreen
    color: "#0a0506"
    title: "Red Bear Greeter"

    function submitLogin() {
        greeterBackend.submitLogin(usernameField.text, passwordField.text)
    }

    // ── Background layers ──────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: "#0a0506"

        Image {
            anchors.fill: parent
            source: greeterBackend.backgroundUrl
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            opacity: 0.75
        }

        // Animated ambient orbs for depth
        Rectangle {
            anchors.fill: parent
            color: "#230a0d"
            opacity: 0.40
        }

        // Orb 1 — top-left warm glow
        Rectangle {
            x: -parent.width * 0.15
            y: -parent.height * 0.10
            width: parent.width * 0.65
            height: parent.width * 0.65
            radius: width / 2
            color: "#ff2a2a"
            opacity: 0.06

            Behavior on opacity {
                NumberAnimation {
                    duration: 4000
                    easing.type: Easing.InOutSine
                }
            }

            SequentialAnimation on opacity {
                loops: Animation.Infinite
                NumberAnimation { to: 0.10; duration: 4000; easing.type: Easing.InOutSine }
                NumberAnimation { to: 0.06; duration: 4000; easing.type: Easing.InOutSine }
            }
        }

        // Orb 2 — bottom-right cool glow
        Rectangle {
            x: parent.width * 0.55
            y: parent.height * 0.45
            width: parent.width * 0.55
            height: parent.width * 0.55
            radius: width / 2
            color: "#ff4444"
            opacity: 0.04

            SequentialAnimation on opacity {
                loops: Animation.Infinite
                NumberAnimation { to: 0.08; duration: 5000; easing.type: Easing.InOutSine }
                NumberAnimation { to: 0.04; duration: 5000; easing.type: Easing.InOutSine }
            }
        }

        // Subtle vignette
        Rectangle {
            anchors.fill: parent
            gradient: RadialGradient {
                centerX: parent.width / 2
                centerY: parent.height / 2
                centerRadius: parent.width * 0.55
                focalX: centerX
                focalY: centerY
                GradientStop { position: 0.0; color: "#00000000" }
                GradientStop { position: 1.0; color: "#cc000000" }
            }
        }
    }

    // ── Clock widget (top-right) ───────────────────────────────────────
    Column {
        anchors.top: parent.top
        anchors.topMargin: 32
        anchors.right: parent.right
        anchors.rightMargin: 40
        spacing: 2
        opacity: 0.85

        Label {
            id: clockLabel
            anchors.right: parent.right
            text: Qt.formatTime(new Date(), "hh:mm")
            font.pixelSize: 42
            font.weight: Font.Light
            color: "#fff8f8"
            renderType: Text.NativeRendering
        }

        Label {
            id: dateLabel
            anchors.right: parent.right
            text: Qt.formatDate(new Date(), "dddd, MMMM d")
            font.pixelSize: 15
            font.weight: Font.Medium
            color: "#e8b8b8"
            renderType: Text.NativeRendering
        }

        Timer {
            interval: 1000
            running: true
            repeat: true
            onTriggered: {
                clockLabel.text = Qt.formatTime(new Date(), "hh:mm")
                dateLabel.text = Qt.formatDate(new Date(), "dddd, MMMM d")
            }
        }
    }

    // ── Main login card ────────────────────────────────────────────────
    Pane {
        id: loginCard
        width: Math.min(parent.width * 0.42, 560)
        anchors.centerIn: parent
        padding: 32

        // Entrance animation
        opacity: 0
        scale: 0.96
        Behavior on opacity { NumberAnimation { duration: 600; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: 600; easing.type: Easing.OutCubic } }

        Component.onCompleted: {
            opacity = 1
            scale = 1
        }

        background: Rectangle {
            radius: 20
            color: "#d9150c0f"
            border.color: "#40ffffff"
            border.width: 1

            // Top accent glow line
            Rectangle {
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: 2
                radius: 2
                color: "#ff5555"
                opacity: 0.6
            }

            // Subtle inner shadow (top edge)
            Rectangle {
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: 40
                radius: parent.radius
                gradient: LinearGradient {
                    startX: 0; startY: 0
                    endX: 0; endY: 40
                    GradientStop { position: 0.0; color: "#18ffffff" }
                    GradientStop { position: 1.0; color: "#00ffffff" }
                }
            }
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 20

            // ── Logo + title block ─────────────────────────────────────
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 172

                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 4
                    source: greeterBackend.iconUrl
                    width: 96
                    height: 96
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }

                Column {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    spacing: 6

                    Label {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "Red Bear OS"
                        font.pixelSize: 28
                        font.bold: true
                        font.letterSpacing: 1.2
                        color: "#fff4f4"
                        renderType: Text.NativeRendering
                    }

                    Label {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: greeterBackend.sessionName
                        font.pixelSize: 14
                        font.weight: Font.Medium
                        color: "#e8a0a0"
                        renderType: Text.NativeRendering
                    }

                    Label {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "Welcome back"
                        font.pixelSize: 12
                        font.italic: true
                        color: "#a07070"
                        renderType: Text.NativeRendering
                    }
                }
            }

            // ── Username field ─────────────────────────────────────────
            Rectangle {
                Layout.fillWidth: true
                height: 48
                radius: 10
                color: usernameField.activeFocus ? "#25ffffff" : "#18ffffff"
                border.color: usernameField.activeFocus ? "#60ff7777" : "#15ffffff"
                border.width: 1

                Behavior on color { ColorAnimation { duration: 200 } }
                Behavior on border.color { ColorAnimation { duration: 200 } }

                TextField {
                    id: usernameField
                    anchors.fill: parent
                    anchors.leftMargin: 16
                    anchors.rightMargin: 16
                    placeholderText: "Username"
                    enabled: !greeterBackend.busy
                    selectByMouse: true
                    color: "#fff8f8"
                    font.pixelSize: 16
                    background: Rectangle { color: "transparent" }
                    onAccepted: passwordField.forceActiveFocus()
                }
            }

            // ── Password field ─────────────────────────────────────────
            Rectangle {
                Layout.fillWidth: true
                height: 48
                radius: 10
                color: passwordField.activeFocus ? "#25ffffff" : "#18ffffff"
                border.color: passwordField.activeFocus ? "#60ff7777" : "#15ffffff"
                border.width: 1

                Behavior on color { ColorAnimation { duration: 200 } }
                Behavior on border.color { ColorAnimation { duration: 200 } }

                TextField {
                    id: passwordField
                    anchors.fill: parent
                    anchors.leftMargin: 16
                    anchors.rightMargin: 16
                    placeholderText: "Password"
                    enabled: !greeterBackend.busy
                    selectByMouse: true
                    echoMode: TextInput.Password
                    color: "#fff8f8"
                    font.pixelSize: 16
                    background: Rectangle { color: "transparent" }
                    onAccepted: root.submitLogin()
                }
            }

            // ── Status message ─────────────────────────────────────────
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: greeterBackend.message
                color: greeterBackend.state === "fatal_error" ? "#ff8888" : "#ffd0d0"
                font.pixelSize: 14
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            // ── Busy indicator ─────────────────────────────────────────
            BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: greeterBackend.busy
                visible: running
                contentItem: Rectangle {
                    implicitWidth: 24
                    implicitHeight: 24
                    color: "transparent"
                    RotationAnimator on rotation {
                        running: parent.parent.running
                        loops: Animation.Infinite
                        from: 0
                        to: 360
                        duration: 1200
                    }
                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        color: "transparent"
                        border.color: "#ff7777"
                        border.width: 2
                        Rectangle {
                            anchors.top: parent.top
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: 4
                            height: 4
                            radius: 2
                            color: "#ff7777"
                        }
                    }
                }
            }

            // ── Action buttons ─────────────────────────────────────────
            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                // Primary: Log In
                Button {
                    id: loginButton
                    Layout.fillWidth: true
                    Layout.preferredHeight: 44
                    text: greeterBackend.busy ? "Authenticating…" : "Log In"
                    enabled: !greeterBackend.busy
                    onClicked: root.submitLogin()

                    contentItem: Label {
                        text: parent.text
                        font.pixelSize: 15
                        font.bold: true
                        font.letterSpacing: 0.5
                        color: parent.enabled ? "#ffffff" : "#886666"
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        renderType: Text.NativeRendering
                    }

                    background: Rectangle {
                        radius: 10
                        gradient: Gradient {
                            GradientStop { position: 0.0; color: loginButton.pressed ? "#cc2222" : (loginButton.hovered ? "#ff4444" : "#e63333") }
                            GradientStop { position: 1.0; color: loginButton.pressed ? "#991111" : (loginButton.hovered ? "#cc2222" : "#b82222") }
                        }
                        border.color: loginButton.hovered ? "#ff8888" : "#cc4444"
                        border.width: 1
                    }
                }

                // Secondary: Shutdown
                Button {
                    id: shutdownButton
                    Layout.preferredWidth: 90
                    Layout.preferredHeight: 44
                    enabled: !greeterBackend.busy
                    onClicked: greeterBackend.requestShutdown()
                    hoverEnabled: true

                    contentItem: Label {
                        text: "Shutdown"
                        font.pixelSize: 13
                        font.bold: true
                        color: shutdownButton.hovered ? "#ffcccc" : "#cc9999"
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        renderType: Text.NativeRendering
                    }

                    background: Rectangle {
                        radius: 10
                        color: shutdownButton.hovered ? "#20ffffff" : "#12ffffff"
                        border.color: shutdownButton.hovered ? "#30ffffff" : "#18ffffff"
                        border.width: 1
                    }
                }

                // Secondary: Reboot
                Button {
                    id: rebootButton
                    Layout.preferredWidth: 80
                    Layout.preferredHeight: 44
                    enabled: !greeterBackend.busy
                    onClicked: greeterBackend.requestReboot()
                    hoverEnabled: true

                    contentItem: Label {
                        text: "Reboot"
                        font.pixelSize: 13
                        font.bold: true
                        color: rebootButton.hovered ? "#ffcccc" : "#cc9999"
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        renderType: Text.NativeRendering
                    }

                    background: Rectangle {
                        radius: 10
                        color: rebootButton.hovered ? "#20ffffff" : "#12ffffff"
                        border.color: rebootButton.hovered ? "#30ffffff" : "#18ffffff"
                        border.width: 1
                    }
                }
            }
        }
    }

    Component.onCompleted: usernameField.forceActiveFocus()
}
