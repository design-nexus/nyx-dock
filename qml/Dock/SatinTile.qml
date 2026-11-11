import QtQuick
import Quickshell
import Nyx.Ui

// The Apps button and the Trash: a satin tile like the app icons, from the theme.
Item {
    id: t

    property var dockWindow
    property string kind: "apps"
    readonly property var w: dockWindow
    readonly property int size: w.iconSize
    readonly property int trash: w.state_.trash || 0

    width: size + 8
    height: size + 12

    Rectangle {
        anchors.fill: parent
        radius: 12
        color: drop.containsDrag ? Style.alpha(Style.accent, 0.18) : area.containsMouse ? Style.alpha(Style.text, 0.07) : "transparent"
        border.width: drop.containsDrag ? 2 : 0
        border.color: Style.accent
    }
    // A soft square with a hairline and a glyph: the Apps button in the accent, the
    // Trash muted until it holds something.
    Rectangle {
        readonly property bool apps: t.kind === "apps"
        x: 4
        y: area.containsMouse ? 2 : 4
        Behavior on y {
            NumberAnimation {
                duration: Style.fast
            }
        }
        width: t.size
        height: t.size
        radius: Math.round(t.size * 0.22)
        color: apps ? Style.alpha(Style.accent, 0.14) : Style.alpha(Style.text, 0.08)
        border.width: 1
        border.color: apps ? Style.alpha(Style.accent, 0.24) : Style.alpha(Style.text, 0.09)
        Icon {
            anchors.centerIn: parent
            size: Math.max(16, Math.floor(t.size / 2))
            recolor: true
            color: parent.apps ? Style.accent : t.trash > 0 ? Style.text : Style.dimText
            name: t.kind === "apps" ? "view-app-grid-symbolic" : (t.trash > 0 ? "user-trash-full-symbolic" : "user-trash-symbolic")
        }
    }
    Tip {
        target: t
        text: t.kind === "apps" ? "Apps" : "Trash · " + t.trash + " items"
        shown: area.containsMouse && !apps.visible
        below: t.w.position === "top"
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: m => {
            if (t.kind === "apps") {
                if (m.button === Qt.RightButton)
                    t.w.openMenu();
                else
                    apps.visible ? apps.close() : apps.open();
            } else if (m.button === Qt.RightButton)
                trashMenu.openAt(t.width / 2, 0);
            else
                t.w.call("OpenTrash", {});
        }
    }
    DropArea {
        id: drop
        anchors.fill: parent
        enabled: t.kind === "trash"
        keys: ["text/uri-list"]
        onDropped: d => {
            t.w.call("TrashFiles", { uris: d.urls.map(u => u.toString()) });
            d.accept();
        }
    }
    Timer {
        interval: 800
        running: t.kind === "apps" && !!Quickshell.env("NYX_DOCK_TEST_APPS")
        onTriggered: apps.open()
    }
    AppsPopup {
        id: apps
        tile: t
        dockWindow: t.w
    }
    Menu {
        id: trashMenu
        anchorItem: t
        property bool armed: false
        entries: [{ id: "open", label: "Open Trash", icon: "folder-open-symbolic" }].concat(t.trash > 0 ? [{ id: "empty", label: armed ? "Empty Trash: this can't be undone" : "Empty Trash…", icon: "user-trash-full-symbolic", danger: true }] : [])
        property bool holding: false
        onVisibleChanged: {
            if (visible !== holding) {
                holding = visible;
                t.w.hold(visible);
            }
            if (!visible)
                armTimer.stop();
        }
        Component.onDestruction: if (holding)
            t.w.hold(false)
        onTriggered: id => {
            if (id === "open")
                t.w.call("OpenTrash", {});
            else if (armed) {
                armed = false;
                t.w.call("EmptyTrash", {});
            } else {
                armed = true;
                armTimer.restart();
                openAt(t.width / 2, 0);
            }
        }
        Timer {
            id: armTimer
            interval: 4000
            onTriggered: trashMenu.armed = false
        }
    }
}
