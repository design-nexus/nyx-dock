import QtQuick
import Quickshell
import Quickshell.Wayland
import Nyx.Ui

// Hover buttons for the focused floating window (window_controls = true): three dots
// at its top-right corner that open into minimize, maximize and close when the pointer
// reaches them. The backend says where (topic `controls`).
PanelWindow {
    id: win

    property var client
    readonly property var place: client.state.controls || null
    readonly property string pinnedScreen: Quickshell.env("NYX_SHELL_SCREEN") || ""
    readonly property bool open: hover.hovered || Quickshell.env("NYX_DOCK_TEST_CONTROLS") === "open"

    readonly property var target: place ? Quickshell.screens.find(s => s.name === place.monitor) || null : null

    visible: target !== null && (pinnedScreen === "" || place.monitor === pinnedScreen)
    screen: target
    anchors.top: true
    anchors.right: true
    margins.top: place ? place.top : 0
    margins.right: place ? place.right : 0
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "nyx-window-controls"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    color: "transparent"
    implicitWidth: pill.width
    implicitHeight: pill.height

    onOpenChanged: if (open)
        client.request("ControlsHover", {}, null)

    Rectangle {
        id: pill
        width: row.implicitWidth + (win.open ? 8 : 12)
        height: row.implicitHeight + (win.open ? 6 : 8)
        radius: 11
        color: win.open ? Style.card : Style.alpha(Style.card, 0.72)
        border.width: 1
        border.color: win.open ? Style.border : Style.alpha(Style.border, 0.6)
        opacity: win.open ? 1 : 0.7
        Behavior on opacity {
            NumberAnimation {
                duration: Style.reducedMotion ? 0 : 120
                easing.type: Easing.OutCubic
            }
        }
        HoverHandler {
            id: hover
        }
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 4
            Repeater {
                model: [
                    { action: "minimize", icon: "window-minimize-symbolic", tip: "Minimize" },
                    { action: "maximize", icon: "window-maximize-symbolic", tip: "Maximize" },
                    { action: "close", icon: "window-close-symbolic", tip: "Close" }
                ]
                delegate: Rectangle {
                    id: b
                    required property var modelData
                    readonly property bool danger: modelData.action === "close"
                    width: win.open ? 24 : 8
                    height: width
                    radius: win.open ? 7 : 4
                    color: !win.open ? Style.dimText : area.containsMouse ? (danger ? Style.danger : Style.alpha(Style.text, 0.08)) : "transparent"
                    Icon {
                        visible: win.open
                        anchors.centerIn: parent
                        name: b.modelData.icon
                        size: 16
                        color: b.danger && area.containsMouse ? Style.onAccent : Style.text
                    }
                    MouseArea {
                        id: area
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: win.client.request("WindowControl", { action: b.modelData.action }, null)
                    }
                    Tip {
                        target: area
                        text: b.modelData.tip
                        shown: win.open && area.containsMouse
                    }
                }
            }
        }
    }
}
