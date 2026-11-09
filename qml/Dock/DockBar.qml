import QtQuick
import Quickshell
import Nyx.Ui

// The bar: the Apps button, pinned and running apps, a line, folders and the Trash.
// Apps can be dragged along it to a new place among the pins, or off it to unpin;
// files dropped on an app open with it, on a folder are copied there, on the Trash go
// to the Trash; folders dropped on the line are added.
Rectangle {
    id: bar

    property var dockWindow
    readonly property var w: dockWindow
    readonly property bool horizontal: w.horizontal
    readonly property int icon: w.iconSize
    readonly property var items: w.mon.items || []
    readonly property var folders: w.state_.folders || []
    // Tiles are keyed so an update keeps them (and their open popups) in place.
    readonly property var itemByKey: items.reduce((m, i) => (m[i.key] = i, m), ({}))
    readonly property var folderByKey: folders.reduce((m, f) => (m[f.index + ":" + f.path] = f, m), ({}))
    readonly property bool showApps: w.global.apps !== false
    readonly property bool showTrash: w.global.trash !== false
    // An app being dragged along the bar: its key and where it would land.
    property string dragKey: ""
    property int dropIndex: -1

    width: lay.implicitWidth + (horizontal ? 16 : 13)
    height: lay.implicitHeight + (horizontal ? 13 : 16)
    radius: Style.radiusDialog
    color: Style.card
    border.width: 1
    border.color: Style.border

    HoverHandler {
        onHoveredChanged: bar.w.hovered = hovered
    }
    function openMenu() {
        menu.open();
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: menu.open()
    }
    Timer {
        interval: 800
        running: !!Quickshell.env("NYX_DOCK_TEST_DOCK_MENU")
        onTriggered: menu.open()
    }
    DockMenu {
        id: menu
        dockWindow: bar.w
        anchorItem: bar
    }

    Grid {
        id: lay
        anchors.centerIn: parent
        flow: bar.horizontal ? Grid.LeftToRight : Grid.TopToBottom
        rows: bar.horizontal ? 1 : 1000
        columns: bar.horizontal ? 1000 : 1
        spacing: 2

        SatinTile {
            visible: bar.showApps
            dockWindow: bar.w
            kind: "apps"
        }
        Repeater {
            model: ScriptModel {
                values: bar.items.map(i => i.key)
            }
            delegate: AppTile {
                required property var modelData
                required property int index
                item: bar.itemByKey[modelData] || ({ key: modelData })
                index_: index
                dockWindow: bar.w
                barItem: bar
            }
        }
        Rectangle {
            id: sep
            visible: bar.folders.length > 0 || bar.showTrash
            width: bar.horizontal ? 1 : bar.icon
            height: bar.horizontal ? bar.icon : 1
            color: folderDrop.containsDrag ? Style.accent : Style.border
            DropArea {
                id: folderDrop
                anchors.fill: parent
                anchors.margins: -10
                keys: ["text/uri-list"]
                onEntered: bar.w.hold(true)
                onExited: bar.w.hold(false)
                onDropped: drop => {
                    bar.w.hold(false);
                    bar.w.call("AddFolders", { uris: drop.urls.map(u => u.toString()) });
                    drop.accept();
                }
            }
        }
        Repeater {
            model: ScriptModel {
                values: bar.folders.map(f => f.index + ":" + f.path)
            }
            delegate: FolderTile {
                required property var modelData
                folder: bar.folderByKey[modelData] || ({ index: -1 })
                dockWindow: bar.w
            }
        }
        SatinTile {
            visible: bar.showTrash
            dockWindow: bar.w
            kind: "trash"
        }
    }
}
