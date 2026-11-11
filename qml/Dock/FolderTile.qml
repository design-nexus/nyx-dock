import QtQuick
import Quickshell
import Nyx.Ui

// A folder on the dock: its newest files stacked (or the folder's icon) with a count;
// opens as a fan, a grid, a list or in the file manager; right-click for how. Files
// dropped on it are copied there; drag it off the dock to remove it.
Item {
    id: t

    property var folder: ({})
    property var dockWindow
    readonly property var w: dockWindow
    readonly property int size: w.iconSize
    readonly property var newest: folder.newest || []

    width: size + 8
    height: size + 12

    Rectangle {
        anchors.fill: parent
        radius: 12
        color: drop.containsDrag ? Style.alpha(Style.accent, 0.18) : area.containsMouse ? Style.alpha(Style.text, 0.07) : "transparent"
        border.width: drop.containsDrag ? 2 : 0
        border.color: Style.accent
    }
    // The stack: up to three newest files as tilted cards.
    Item {
        x: 4
        y: area.containsMouse ? 2 : 4
        Behavior on y {
            NumberAnimation {
                duration: Style.fast
            }
        }
        width: t.size
        height: t.size
        visible: t.folder.look === "stack" && t.newest.length > 0
        Repeater {
            model: t.newest.slice(0, 3).reverse()
            delegate: Rectangle {
                required property var modelData
                required property int index
                readonly property int k: Math.min(2, t.newest.length - 1) - index
                width: t.size * 3 / 4
                height: t.size * 7 / 8
                anchors.centerIn: parent
                rotation: [0, 6, -10][k]
                transform: Translate {
                    x: [0, 4, -4][k]
                    y: [2, 0, 0][k]
                }
                radius: 4
                color: Style.raised
                border.width: 1
                border.color: Style.border
                clip: true
                Image {
                    anchors.fill: parent
                    anchors.margins: 1
                    visible: !!modelData.thumb || modelData.image
                    source: modelData.thumb ? "file://" + modelData.thumb : (modelData.image ? modelData.uri : "")
                    sourceSize: Qt.size(128, 128)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                Icon {
                    anchors.centerIn: parent
                    visible: !modelData.thumb && !modelData.image
                    size: parent.width * 0.6
                    name: modelData.icon
                }
            }
        }
    }
    Icon {
        x: 4
        y: area.containsMouse ? 2 : 4
        Behavior on y {
            NumberAnimation {
                duration: Style.fast
            }
        }
        visible: !(t.folder.look === "stack" && t.newest.length > 0)
        size: t.size
        name: t.folder.icon || "folder"
    }
    Rectangle {
        visible: !!t.folder.show_count && t.folder.count > 0
        anchors.right: parent.right
        anchors.top: parent.top
        height: 18
        width: Math.max(18, countText.implicitWidth + 10)
        radius: 9
        color: Style.raised
        border.width: 2
        border.color: Style.popover
        Label {
            id: countText
            anchors.centerIn: parent
            text: t.folder.count > 99 ? "99+" : String(t.folder.count)
            size: 11
            font.weight: Font.DemiBold
        }
    }
    Tip {
        target: t
        text: (t.folder.name || "") + " · " + (t.folder.count || 0) + " items"
        shown: area.containsMouse && !view.visible
        below: t.w.position === "top"
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        drag.target: ghost
        drag.threshold: 8
        onClicked: m => {
            if (m.button === Qt.RightButton)
                menu.openAt(t.width / 2, 0);
            else if (t.folder.open === "files")
                t.w.call("OpenDefault", { uri: t.folder.uri });
            else
                view.open();
        }
        onReleased: {
            if (!drag.active)
                return;
            if (!ghost.Drag.target)
                t.w.call("RemoveFolder", { index: t.folder.index });
            ghost.x = 0;
            ghost.y = 0;
        }
    }
    Item {
        id: ghost
        width: t.size
        height: t.size
        Drag.active: area.drag.active
        Drag.keys: ["nyx-dock-folder"]
    }
    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["text/uri-list"]
        onDropped: d => {
            t.w.call("CopyInto", { path: t.folder.path, uris: d.urls.map(u => u.toString()) });
            d.accept();
        }
    }
    Timer {
        interval: 800
        running: t.folder.index === 0
        onTriggered: {
            const v = Quickshell.env("NYX_DOCK_TEST_FOLDER");
            if (v === "menu")
                menu.openAt(t.width / 2, 0);
            else if (v)
                view.open();
        }
    }
    FolderPopup {
        id: view
        tile: t
        dockWindow: t.w
    }
    Menu {
        id: menu
        anchorItem: t
        entries: [
            { heading: "Sort by" },
            { id: "sort:date", label: "Date Added", checked: t.folder.sort === "date" },
            { id: "sort:name", label: "Name", checked: t.folder.sort === "name" },
            { id: "sort:kind", label: "Kind", checked: t.folder.sort === "kind" },
            { heading: "Display as" },
            { id: "look:stack", label: "Stack", checked: t.folder.look === "stack" },
            { id: "look:folder", label: "Folder", checked: t.folder.look === "folder" },
            { heading: "View content as" },
            { id: "open:fan", label: "Fan", checked: t.folder.open === "fan" },
            { id: "open:grid", label: "Grid", checked: t.folder.open === "grid" },
            { id: "open:list", label: "List", checked: t.folder.open === "list" },
            { id: "open:files", label: "Files", checked: t.folder.open === "files" },
            { separator: true },
            { id: "count", label: "Show item count", checked: !!t.folder.show_count },
            { separator: true },
            { id: "files", label: "Open in Files", icon: "folder-open-symbolic" },
            { id: "remove", label: "Remove from Dock", icon: "list-remove-symbolic" }
        ]
        onVisibleChanged: t.w.hold(visible)
        onTriggered: id => {
            if (id === "files")
                t.w.call("OpenDefault", { uri: t.folder.uri });
            else if (id === "remove")
                t.w.call("RemoveFolder", { index: t.folder.index });
            else if (id === "count")
                t.w.call("SetFolder", { index: t.folder.index, key: "count", value: !t.folder.show_count });
            else {
                const [k, v] = id.split(":");
                t.w.call("SetFolder", { index: t.folder.index, key: k, value: v });
            }
        }
    }
}
