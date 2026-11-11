import QtQuick
import QtQuick.Effects
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

    // The popover surface, 22 px corners, 6 px above the tiles, 8 at the ends, 3 below
    // (their marks sit low), and a soft shadow; the window leaves room for it.
    width: lay.implicitWidth + (horizontal ? 16 : 12)
    height: lay.implicitHeight + (horizontal ? 9 : 16)
    radius: 22
    color: "transparent"

    MultiEffect {
        anchors.fill: surface
        source: surface
        z: -2
        shadowEnabled: true
        shadowColor: Style.alpha(Style.shadow, 0.55 * Style.shadowIntensity / 0.5)
        shadowBlur: 1.0
        blurMax: 40
        shadowVerticalOffset: 10
        shadowHorizontalOffset: 0
    }
    Rectangle {
        id: surface
        anchors.fill: parent
        z: -1
        radius: bar.radius
        color: Style.popover
        border.width: 1
        border.color: Style.light ? Style.border : Style.alpha(Style.text, 0.09)
    }

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

    // A line between groups: 1 px, inset from the tile's top and its marks.
    component Line: Item {
        property bool lit: false
        width: bar.horizontal ? 13 : bar.icon + 8
        height: bar.horizontal ? bar.icon + 12 : 13
        Rectangle {
            x: bar.horizontal ? 6 : 10
            y: bar.horizontal ? 10 : 6
            width: bar.horizontal ? 1 : parent.width - 20
            height: bar.horizontal ? parent.height - 24 : 1
            color: parent.lit ? Style.accent : Style.alpha(Style.text, 0.14)
        }
    }

    Grid {
        id: lay
        x: bar.horizontal ? 8 : 6
        y: bar.horizontal ? 6 : 8
        flow: bar.horizontal ? Grid.LeftToRight : Grid.TopToBottom
        rows: bar.horizontal ? 1 : 1000
        columns: bar.horizontal ? 1000 : 1
        spacing: 3

        SatinTile {
            visible: bar.showApps
            dockWindow: bar.w
            kind: "apps"
        }
        Line {
            visible: bar.showApps && bar.items.length > 0
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
                leadLine: !item.pinned && index > 0 && !!(bar.items[index - 1] || {}).pinned
            }
        }
        Line {
            id: sep
            visible: bar.folders.length > 0 || bar.showTrash
            lit: folderDrop.containsDrag
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
