import QtQuick
import Nyx.Ui

// A folder's files: a fan (the newest eight, curving up), a grid or a list (all of
// them, scrolling), with the folder's name, its count, the sort and Open in Files.
// Click a file to open it; drag one out to another app.
DockPopup {
    id: p

    property var tile
    readonly property var folder: tile.folder
    property var entries: []
    property string sort: folder.sort || "date"
    readonly property string kind: folder.open || "grid"

    anchorTo: tile
    pad: 10
    bare: kind === "fan"

    function load() {
        w.client.request("Folder", { index: folder.index, sort: sort }, ok => {
            p.entries = ok || [];
            p.ready();
        });
    }
    onPreparing: {
        waiting = true;
        load();
    }
    onSortChanged: if (visible)
        load()
    function openFile(e) {
        p.close();
        w.call("OpenDefault", { uri: e.uri });
    }

    component Thumb: Rectangle {
        property var entry: ({})
        property int box: 48
        // The fan's: a framed picture over the desktop.
        property bool framed: false
        width: box
        height: box * 7 / 8
        radius: framed ? 8 : 6
        color: framed ? Style.popover : Style.alpha(Style.text, 0.05)
        border.width: framed ? 1 : 0
        border.color: Style.border
        clip: true
        Image {
            anchors.fill: parent
            visible: status === Image.Ready
            source: parent.entry.thumb ? "file://" + parent.entry.thumb : (parent.entry.image ? parent.entry.uri : "")
            sourceSize: Qt.size(160, 160)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Icon {
            anchors.centerIn: parent
            visible: !parent.entry.thumb && !parent.entry.image
            size: parent.width * 2 / 3
            name: parent.entry.icon || "text-x-generic"
        }
        Drag.dragType: Drag.Automatic
        Drag.mimeData: ({ "text/uri-list": entry.uri })
        Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
    }

    // How far a fan row sits from the edge: a gentle curve, the top row nearest it.
    function arc(i) {
        return 3 * i * i;
    }
    readonly property var fanned: entries.slice(0, 8)
    readonly property int fanWidth: 266 + arc(fanned.length) - arc(1)

    component FanPill: Rectangle {
        property string text: ""
        property bool hot: false
        width: pillLabel.width + 18
        height: pillLabel.implicitHeight + 8
        radius: 7
        color: hot ? Style.accent : Style.popover
        border.width: hot ? 0 : 1
        border.color: Style.border
        Label {
            id: pillLabel
            anchors.centerIn: parent
            width: Math.min(implicitWidth, 190)
            text: parent.text
            size: 12
            elide: Text.ElideMiddle
            color: parent.hot ? Style.onAccent : Style.text
        }
    }

    Column {
        width: p.kind === "fan" ? p.fanWidth : p.kind === "list" ? 320 : 340
        spacing: 8

        // Grid and list: a header.
        Row {
            visible: p.kind !== "fan"
            width: parent.width
            spacing: 6
            Column {
                width: parent.width - 2 * 34 - 12
                Label {
                    text: p.folder.name || ""
                    font.weight: Font.DemiBold
                }
                Label {
                    text: p.entries.length + " items"
                    role: "caption"
                    dim: true
                }
            }
            IconButton {
                icon: "view-sort-descending-symbolic"
                tooltip: ({ date: "Newest first", name: "By name", kind: "By kind" })[p.sort]
                onClicked: {
                    const next = { date: "name", name: "kind", kind: "date" }[p.sort];
                    p.sort = next;
                    p.w.call("SetFolder", { index: p.folder.index, key: "sort", value: next });
                }
            }
            IconButton {
                icon: "folder-open-symbolic"
                tooltip: "Open in Files"
                onClicked: {
                    p.close();
                    p.w.call("OpenDefault", { uri: p.folder.uri });
                }
            }
        }
        Label {
            visible: p.kind !== "fan" && p.entries.length === 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            topPadding: 20
            bottomPadding: 20
            text: "Nothing here"
            dim: true
        }

        // Fan: no panel; Open in Files on top, then the newest eight, oldest first,
        // each a name and a picture, bending away from the edge as they come down.
        Column {
            visible: p.kind === "fan"
            width: parent.width
            spacing: 8
            Item {
                width: parent.width
                height: 42
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    FanPill {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Open in Files · " + p.entries.length + " items"
                        hot: openAll.containsMouse
                    }
                    Rectangle {
                        width: 48
                        height: 42
                        radius: 8
                        color: Style.popover
                        border.width: 1
                        border.color: Style.border
                        Icon {
                            anchors.centerIn: parent
                            name: "folder-open-symbolic"
                            size: 24
                            color: Style.text
                        }
                    }
                }
                MouseArea {
                    id: openAll
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        p.close();
                        p.w.call("OpenDefault", { uri: p.folder.uri });
                    }
                }
            }
            Repeater {
                model: p.kind === "fan" ? p.fanned.slice().reverse() : []
                delegate: Item {
                    id: fanRow
                    required property var modelData
                    required property int index
                    // Its place counted from the newest (0).
                    readonly property int i: p.fanned.length - 1 - index
                    width: parent.width
                    height: 42
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: p.arc(p.fanned.length) - p.arc(fanRow.i + 1)
                        spacing: 10
                        FanPill {
                            anchors.verticalCenter: parent.verticalCenter
                            text: fanRow.modelData.name
                            hot: fanArea.containsMouse
                        }
                        Thumb {
                            entry: fanRow.modelData
                            box: 48
                            framed: true
                        }
                    }
                    MouseArea {
                        id: fanArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: p.openFile(fanRow.modelData)
                    }
                }
            }
        }

        // Grid and list: everything, scrolling.
        Flickable {
            visible: p.kind !== "fan" && p.entries.length > 0
            width: parent.width
            height: Math.min(contentHeight, Math.max(200, Math.min(p.kind === "grid" ? 460 : 460, (p.w.screen ? p.w.screen.height : 800) * 0.6)))
            clip: true
            contentHeight: p.kind === "grid" ? grid.height : listCol.height
            boundsBehavior: Flickable.StopAtBounds
            Grid {
                id: grid
                visible: p.kind === "grid"
                columns: 4
                spacing: 6
                Repeater {
                    model: p.kind === "grid" ? p.entries : []
                    delegate: Rectangle {
                        required property var modelData
                        width: 80
                        height: 92
                        radius: Style.radiusSmall
                        color: gridArea.containsMouse ? Style.rowHover : "transparent"
                        Thumb {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 4
                            entry: parent.modelData
                            box: 64
                        }
                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 62
                            width: parent.width - 6
                            horizontalAlignment: Text.AlignHCenter
                            text: parent.modelData.name
                            elide: Text.ElideMiddle
                            role: "caption"
                        }
                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 76
                            text: parent.modelData.age
                            role: "caption"
                            dim: true
                        }
                        MouseArea {
                            id: gridArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: p.openFile(parent.modelData)
                        }
                    }
                }
            }
            Column {
                id: listCol
                visible: p.kind === "list"
                width: parent.width
                Repeater {
                    model: p.kind === "list" ? p.entries : []
                    delegate: Rectangle {
                        required property var modelData
                        width: listCol.width
                        height: 34
                        radius: Style.radiusSmall
                        color: listArea.containsMouse ? Style.rowHover : "transparent"
                        Thumb {
                            x: 4
                            anchors.verticalCenter: parent.verticalCenter
                            entry: parent.modelData
                            box: 28
                        }
                        Label {
                            x: 40
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 40 - age.width - 12
                            text: parent.modelData.name
                            elide: Text.ElideMiddle
                        }
                        Label {
                            id: age
                            anchors.right: parent.right
                            anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.modelData.age
                            role: "caption"
                            dim: true
                        }
                        MouseArea {
                            id: listArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: p.openFile(parent.modelData)
                        }
                    }
                }
            }
        }
    }
}
