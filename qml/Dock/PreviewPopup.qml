import QtQuick
import Nyx.Ui

// The windows of a running app, pictured: click one to go to it, close it from its
// card. Opens after a rest on the icon, closes 300 ms after the pointer leaves both.
DockPopup {
    id: p

    property var tile
    readonly property var windows: tile.item.windows || []
    readonly property bool vertical: !w.horizontal
    property var shots: ({})
    // Cards are 212 wide, smaller when that many don't fit across the screen.
    readonly property real room: w.screen ? (vertical ? w.screen.height : w.screen.width) - 48 : 1280
    readonly property real cardW: vertical ? 212 : Math.max(96, Math.min(212, room / Math.max(1, windows.length) - 8))
    readonly property real cardH: vertical ? Math.min(128, Math.max(58, room / Math.max(1, windows.length) - 38)) : cardW * 128 / 212
    property bool hoveredInside: inside.hovered

    anchorTo: tile
    pad: 8
    grab: false

    function leaveSoon() {
        leave.restart();
    }
    Timer {
        id: leave
        interval: 300
        onTriggered: if (!p.tile.hovered && !p.hoveredInside)
            p.close()
    }
    onHoveredInsideChanged: if (!hoveredInside)
        leave.restart()
    onPopupOpened: {
        const ids = windows.map(x => x.stable_id).filter(x => x);
        w.client.request("Shots", { ids: ids, width: 340, height: 205 }, ok => {
            if (ok)
                p.shots = ok;
        });
    }

    Connections {
        target: p.w.client
        enabled: p.visible
        function onEvent(ev, body) {
            if (ev === "Shot" && p.windows.some(x => x.stable_id === body.id))
                p.shots = Object.assign({}, p.shots, { [body.id]: body.path });
        }
    }
    HoverHandler {
        id: inside
    }
    Grid {
        flow: p.vertical ? Grid.TopToBottom : Grid.LeftToRight
        rows: p.vertical ? 1000 : 1
        columns: p.vertical ? 1 : 1000
        spacing: 8
        Repeater {
            model: p.windows
            delegate: Rectangle {
                id: c
                required property var modelData
                width: p.cardW
                height: p.cardH + 30
                radius: Style.radiusCard
                color: cardArea.containsMouse ? Style.alpha(Style.text, 0.06) : "transparent"
                Rectangle {
                    x: 0
                    y: 0
                    width: p.cardW
                    height: p.cardH
                    radius: Style.radiusSmall
                    color: Style.alpha(Style.text, 0.05)
                    clip: true
                    Image {
                        anchors.fill: parent
                        visible: status === Image.Ready
                        source: p.shots[c.modelData.stable_id] ? "file://" + p.shots[c.modelData.stable_id] : ""
                        fillMode: Image.PreserveAspectFit
                        cache: false
                        asynchronous: true
                    }
                    Icon {
                        anchors.centerIn: parent
                        visible: !p.shots[c.modelData.stable_id]
                        size: 48
                        name: p.tile.item.icon
                    }
                }
                Row {
                    x: 4
                    y: 134
                    width: parent.width - 8
                    spacing: 6
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        size: 16
                        name: p.tile.item.icon
                    }
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 16 - 6 - ws.width - 6
                        text: c.modelData.title
                        size: 12
                    }
                    Rectangle {
                        id: ws
                        anchors.verticalCenter: parent.verticalCenter
                        readonly property string label: c.modelData.minimized ? "–" : (c.modelData.special ? "" : String(c.modelData.workspace))
                        visible: label !== ""
                        width: Math.max(18, wsl.implicitWidth + 8)
                        height: 18
                        radius: 9
                        color: c.modelData.current ? Style.alpha(Style.accent, 0.25) : Style.alpha(Style.text, 0.08)
                        Label {
                            id: wsl
                            anchors.centerIn: parent
                            text: ws.label
                            role: "caption"
                        }
                    }
                }
                MouseArea {
                    id: cardArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        p.close();
                        p.w.call("FocusWindow", { address: c.modelData.address });
                    }
                }
                IconButton {
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 4
                    visible: cardArea.containsMouse || hovered
                    width: 26
                    height: 26
                    icon: "window-close-symbolic"
                    tooltip: "Close"
                    danger: true
                    color: Style.alpha(Style.popover, 0.9)
                    onClicked: {
                        p.w.call("CloseWindow", { address: c.modelData.address });
                        if (p.windows.length <= 1)
                            p.close();
                    }
                }
            }
        }
    }
}
