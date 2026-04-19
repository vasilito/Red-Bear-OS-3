import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: root
    visible: true
    visibility: Window.FullScreen
    color: "#11090a"
    title: "Red Bear Greeter"

    function submitLogin() {
        greeterBackend.submitLogin(usernameField.text, passwordField.text)
    }

    Rectangle {
        anchors.fill: parent
        color: "#11090a"

        Image {
            anchors.fill: parent
            source: greeterBackend.backgroundUrl
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            opacity: 0.88
        }

        Rectangle {
            anchors.fill: parent
            color: "#230a0d"
            opacity: 0.45
        }
    }

    Pane {
        width: Math.min(parent.width * 0.42, 620)
        anchors.centerIn: parent
        padding: 28

        background: Rectangle {
            radius: 18
            color: "#cc150c0f"
            border.color: "#66f7d7d7"
            border.width: 1
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 18

            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 156

                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 2
                    source: greeterBackend.iconUrl
                    width: 108
                    height: 108
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }

                Column {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    spacing: 4

                    Label {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "Red Bear OS"
                        font.pixelSize: 26
                        font.bold: true
                        color: "#fff4f4"
                    }

                    Label {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: greeterBackend.sessionName
                        font.pixelSize: 15
                        color: "#f1c5c5"
                    }
                }
            }

            TextField {
                id: usernameField
                Layout.fillWidth: true
                placeholderText: "Username"
                enabled: !greeterBackend.busy
                selectByMouse: true
                color: "#fff8f8"
                font.pixelSize: 18
                onAccepted: passwordField.forceActiveFocus()
            }

            TextField {
                id: passwordField
                Layout.fillWidth: true
                placeholderText: "Password"
                enabled: !greeterBackend.busy
                selectByMouse: true
                echoMode: TextInput.Password
                color: "#fff8f8"
                font.pixelSize: 18
                onAccepted: root.submitLogin()
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: greeterBackend.message
                color: greeterBackend.state === "fatal_error" ? "#ffb4b4" : "#ffe7e7"
                font.pixelSize: 15
            }

            BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: greeterBackend.busy
                visible: running
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                Button {
                    Layout.fillWidth: true
                    text: greeterBackend.busy ? "Working…" : "Log In"
                    enabled: !greeterBackend.busy
                    onClicked: root.submitLogin()
                }

                Button {
                    text: "Shutdown"
                    enabled: !greeterBackend.busy
                    onClicked: greeterBackend.requestShutdown()
                }

                Button {
                    text: "Reboot"
                    enabled: !greeterBackend.busy
                    onClicked: greeterBackend.requestReboot()
                }
            }
        }
    }

    Component.onCompleted: usernameField.forceActiveFocus()
}
