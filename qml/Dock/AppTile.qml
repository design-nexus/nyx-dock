import QtQuick
import Quickshell
import Nyx.Ui

// One app on the dock: its icon, a dot (two for more windows; an accent bar for the
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

    width: size + 8
    height: size + 10

    Rectangle {
        anchors.fill: parent
        radius: Style.radiusSmall
        color: drop.containsDrag ? Style.alpha(Style.accent, 0.16) : t.hovered ? Style.alpha(Style.text, 0.06) : "transparent"
        Behavior on color {
            ColorAnimation {
                duration: Style.fast
            }
        }
    }
    Icon {
        id: icon
        x: 4
        y: 2
        size: t.size
        name: t.item.icon || "application-x-executable"
        recolor: false
        opacity: area.drag.active ? 0.3 : 1
    }
    Rectangle {
        visible: t.item.badge !== null && t.item.badge !== undefined
        anchors.right: icon.right
        anchors.top: icon.top
        anchors.rightMargin: -3
        anchors.topMargin: -2
        height: 18
        width: Math.max(18, badgeText.implicitWidth + 10)
        radius: 9
        color: Style.danger
        border.width: 2
        border.color: Style.card
        Label {
            id: badgeText
            anchors.centerIn: parent
            text: t.item.badge > 99 ? "99+" : String(t.item.badge)
            color: "white"
            size: 11
            font.weight: Font.DemiBold
        }
    }
    ProgressBar {
        visible: t.item.progress !== null && t.item.progress !== undefined
        anchors.left: icon.left
        anchors.right: icon.right
        anchors.bottom: icon.bottom
        anchors.margins: 2
        height: 3
        value: t.item.progress || 0
    }
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 1
        spacing: 3
        Repeater {
            model: Math.min(2, t.windows)
            Rectangle {
                required property int index
                width: t.item.focused && index === 0 ? 12 : 4
                height: 4
                radius: 2
                color: t.item.urgent ? Style.warning : t.item.focused ? Style.accent : Style.dimText
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
            ghost.x = icon.x;
            ghost.y = icon.y;
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
            ghost.x = icon.x;
            ghost.y = icon.y;
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
        onVisibleChanged: t.w.hold(visible)
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
