import QtQuick
import QtQuick.Effects
import Quickshell
import Nyx.Ui

// One app on the dock: its icon, a mark per window (up to three; an accent bar for the
// app you're in), its badge and progress. Click: open it or bring its window forward
// (again: the next window); middle-click: a new window; right-click: its card; rest
// the pointer: its windows' previews. Drag it along the dock to pin it there, or off
// to unpin.
Item {
    id: t

    property var item: ({})
    property int index_: 0
    property var dockWindow
    property var barItem
    readonly property var w: dockWindow
    readonly property int size: w.iconSize
    readonly property int windows: (item.windows || []).length
    readonly property bool hovered: area.containsMouse
    // The first running app after the pins: a faint line before it.
    property bool leadLine: false
    readonly property int lead: leadLine ? 7 : 0
    readonly property bool horizontal: w.horizontal

    width: size + 8 + (horizontal ? lead : 0)
    height: size + 12 + (horizontal ? 0 : lead)

    Rectangle {
        visible: t.leadLine
        x: t.horizontal ? 1 : 18
        y: t.horizontal ? 18 : 1
        width: t.horizontal ? 1 : t.size + 8 - 40
        height: t.horizontal ? t.height - 40 : 1
        color: Style.alpha(Style.text, 0.08)
    }
    Item {
        id: tile
        x: t.horizontal ? t.lead : 0
        y: t.horizontal ? 0 : t.lead
        width: t.size + 8
        height: t.size + 12

        Rectangle {
            anchors.fill: parent
            radius: 12
            color: drop.containsDrag ? Style.alpha(Style.accent, 0.18) : t.hovered ? Style.alpha(Style.text, 0.07) : "transparent"
            border.width: drop.containsDrag ? 2 : 0
            border.color: Style.accent
            Behavior on color {
                ColorAnimation {
                    duration: Style.fast
                }
            }
        }
        // Asking for attention: a warm glow behind the icon.
        MultiEffect {
            anchors.fill: icon
            source: icon
            visible: !!t.item.urgent
            shadowEnabled: true
            shadowColor: Style.alpha(Style.warning, 0.55)
            shadowBlur: 0.6
            blurMax: 16
            shadowVerticalOffset: 0
            shadowHorizontalOffset: 0
        }
        Icon {
            id: icon
            x: 4
            y: t.hovered && !area.drag.active ? 2 : 4
            Behavior on y {
                NumberAnimation {
                    duration: Style.fast
                }
            }
            size: t.size
            name: t.item.icon || "application-x-executable"
            recolor: false
            opacity: area.drag.active ? 0.3 : 1
        }
        Rectangle {
            visible: t.item.badge !== null && t.item.badge !== undefined
            anchors.right: parent.right
            anchors.top: parent.top
            height: 18
            width: Math.max(18, badgeText.implicitWidth + 10)
            radius: 9
            color: Style.danger
            border.width: 2
            border.color: Style.panel
            Label {
                id: badgeText
                anchors.centerIn: parent
                text: t.item.badge > 99 ? "99+" : String(t.item.badge)
                color: "white"
                size: 11
                font.weight: Font.Bold
            }
        }
        // Progress: an accent bar on a dark track across the icon's foot.
        Rectangle {
            visible: t.item.progress !== null && t.item.progress !== undefined
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            anchors.bottomMargin: 11
            height: 5
            radius: 3
            color: Qt.rgba(0, 0, 0, 0.55)
            border.width: 1
            border.color: Qt.rgba(1, 1, 1, 0.15)
            clip: true
            Rectangle {
                width: parent.width * Math.max(0, Math.min(1, t.item.progress || 0))
                height: parent.height
                radius: 3
                color: Style.accent
            }
        }
        // A mark per window, up to three; the app you're in gets one accent bar.
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 2
            spacing: 3
            Repeater {
                model: t.item.focused ? Math.min(1, t.windows) : Math.min(3, t.windows)
                Rectangle {
                    width: t.item.focused ? 16 : 4
                    height: 4
                    radius: 2
                    color: t.item.urgent ? Style.warning : t.item.focused ? Style.accent : Style.alpha(Style.text, 0.55)
                }
            }
        }
    }

    Tip {
        target: t
        text: t.item.name || ""
        shown: t.hovered && !(t.w.global.previews && t.windows > 0) && !area.drag.active
        below: t.w.position === "top"
    }

    Timer {
        id: previewWait
        interval: preview.visible ? 0 : (t.w.global.preview_delay_ms || 350)
        onTriggered: if (t.hovered && t.windows > 0 && t.w.global.previews !== false && !card.visible)
            preview.open()
    }
    onHoveredChanged: {
        if (hovered)
            previewWait.restart();
        else {
            previewWait.stop();
            preview.leaveSoon();
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        drag.target: t.item.app ? ghost : null
        drag.threshold: 8
        onPressed: m => {
            ghost.x = tile.x + icon.x;
            ghost.y = tile.y + icon.y;
        }
        onClicked: m => {
            if (m.button === Qt.RightButton) {
                preview.close();
                card.open();
            } else if (m.button === Qt.MiddleButton) {
                if (t.item.app)
                    t.w.call("NewInstance", { key: t.item.key });
            } else {
                preview.close();
                t.w.call("Activate", { key: t.item.key });
            }
        }
        onReleased: {
            if (!drag.active)
                return;
            const target = ghost.Drag.target;
            ghost.Drag.drop();
            // Let go off the dock: a pinned app leaves it.
            if (!target && t.item.pinned)
                t.w.call("Pin", { key: t.item.key, pinned: false });
            ghost.x = tile.x + icon.x;
            ghost.y = tile.y + icon.y;
        }
    }
    Item {
        id: ghost
        width: t.size
        height: t.size
        visible: area.drag.active
        Drag.active: area.drag.active
        Drag.keys: ["nyx-dock-app"]
        Drag.hotSpot.x: width / 2
        Drag.hotSpot.y: height / 2
        property string key: t.item.key
        Icon {
            anchors.fill: parent
            size: t.size
            name: t.item.icon || "application-x-executable"
        }
        // Only a real drag holds the dock: a new tile's ghost starts visible and hides,
        // and that release took another popup's hold (the Apps panel's).
        property bool holding: false
        onVisibleChanged: if (visible !== holding) {
            holding = visible;
            t.w.hold(visible);
        }
        Component.onDestruction: if (holding)
            t.w.hold(false)
    }
    // Another app dropped here takes this place among the pins; files open with this app.
    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["nyx-dock-app", "text/uri-list"]
        onDropped: d => {
            if (d.source && d.source.key !== undefined) {
                if (d.source.key !== t.item.key) {
                    const pins = (t.w.mon.items || []).filter(i => i.pinned);
                    const at = t.item.pinned ? pins.findIndex(i => i.key === t.item.key) : pins.length;
                    t.w.call("Pin", { key: d.source.key, pinned: true, at: Math.max(0, at) });
                }
                d.accept();
            } else if (d.hasUrls && t.item.app) {
                t.w.call("OpenWith", { key: t.item.key, uris: d.urls.map(u => u.toString()) });
                d.accept();
            }
        }
    }

    // Screenshot hooks, as the GTK dock's: NYX_DOCK_TEST_CARD=<app>, NYX_DOCK_TEST_PREVIEW=<app>.
    Timer {
        interval: 800
        running: true
        onTriggered: {
            if (Quickshell.env("NYX_DOCK_TEST_CARD") === t.item.key)
                card.open();
            else if (Quickshell.env("NYX_DOCK_TEST_PREVIEW") === t.item.key)
                preview.open();
        }
    }
    PreviewPopup {
        id: preview
        tile: t
        dockWindow: t.w
    }
    CardPopup {
        id: card
        tile: t
        dockWindow: t.w
    }
}
