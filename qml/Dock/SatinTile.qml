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
    height: size + 10

    Rectangle {
        anchors.fill: parent
        radius: Style.radiusSmall
        color: drop.containsDrag ? Style.alpha(Style.accent, 0.16) : area.containsMouse ? Style.alpha(Style.text, 0.06) : "transparent"
    }
    // As the GTK dock's .dock-satin: a soft square, a hairline, a muted glyph.
    Rectangle {
        x: 4
        y: 2
        width: t.size
        height: t.size
        radius: 11
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Style.mix(Style.bg, Style.text, 0.1)
            }
            GradientStop {
                position: 1
                color: Style.mix(Style.bg, Style.text, 0.03)
            }
        }
        border.width: 1
        border.color: Style.alpha(Style.text, 0.1)
        Icon {
            anchors.centerIn: parent
            size: Math.max(16, Math.floor(t.size / 2))
            recolor: true
            color: Style.mix(Style.text, Style.bg, 0.32)
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
        onVisibleChanged: {
            t.w.hold(visible);
            if (!visible)
                armTimer.stop();
        }
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
