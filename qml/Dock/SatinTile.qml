import QtQuick
import Quickshell
import Nyx.Ui
import Nyx.Theme

// The Apps button and the Trash: Design 2.0 tiles like the app icons, from the theme.
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
    // A Design 2.0 tile like the app icons beside it: the Apps button in the accent, the
    // Trash in graphite, lighter while it holds something.
    function square(x, y, s, r) {
        return "M" + (x + r) + " " + y + "h" + (s - 2 * r) + "a" + r + " " + r + " 0 0 1 " + r + " " + r + "v" + (s - 2 * r)
            + "a" + r + " " + r + " 0 0 1 -" + r + " " + r + "h-" + (s - 2 * r) + "a" + r + " " + r + " 0 0 1 -" + r + " -" + r
            + "v-" + (s - 2 * r) + "a" + r + " " + r + " 0 0 1 " + r + " -" + r + "z";
    }
    readonly property var appsGlyph: ({
        back: square(69, 33, 26, 7) + square(33, 69, 26, 7),
        front: square(33, 33, 26, 7) + square(69, 69, 26, 7),
        detail: ""
    })
    readonly property string lid: "M34 38h60a4 4 0 0 1 0 8H34a4 4 0 0 1 0-8zM54 29h20a4 4 0 0 1 4 4v5H50v-5a4 4 0 0 1 4-4z"
    readonly property var trashGlyph: ({
        back: (trash > 0 ? "M47 17l24-5 5 22-24 5z" : "") + lid,
        front: "M38 51h52l-4 41a8 8 0 0 1-8 7H50a8 8 0 0 1-8-7z",
        detail: "M55 61v27M64 61v27M73 61v27"
    })
    readonly property color graphite: Style.toColor((NyxTheme.swatches || {}).graphite || Style.dimText)
    DesignTile {
        x: 4
        y: area.containsMouse ? 2 : 4
        Behavior on y {
            NumberAnimation {
                duration: Style.fast
            }
        }
        size: t.size
        glyph: t.kind === "apps" ? t.appsGlyph : t.trashGlyph
        tint: t.kind === "apps" ? Style.accent : t.trash > 0 ? Style.mix(t.graphite, Style.text, 0.35) : t.graphite
    }
    Tip {
        target: t
        text: t.kind === "apps" ? "Apps" : "Trash · " + t.trash + " items"
        shown: area.containsMouse && !apps.visible
        below: t.w.position === "top"
    }
    function activate() {
        if (t.kind === "apps")
            apps.visible ? apps.close() : apps.open();
        else
            t.w.call("OpenTrash", {});
    }
    function openMenu() {
        if (t.kind === "apps")
            t.w.openMenu();
        else
            trashMenu.openAt(t.width / 2, 0);
    }
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: t.kind === "apps" ? "Apps" : "Trash"
    Accessible.description: t.kind === "trash" ? t.trash + " items" : ""
    Accessible.onPressAction: activate()
    Keys.onPressed: e => {
        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) {
            activate();
            e.accepted = true;
        } else if (e.key === Qt.Key_Menu || (e.key === Qt.Key_F10 && (e.modifiers & Qt.ShiftModifier))) {
            openMenu();
            e.accepted = true;
        }
    }
    FocusRing {
        radius_: 14
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: m => m.button === Qt.RightButton ? t.openMenu() : t.activate()
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
