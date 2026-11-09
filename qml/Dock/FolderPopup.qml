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

    function load() {
        w.client.request("Folder", { index: folder.index, sort: sort }, ok => p.entries = ok || []);
    }
    onPopupOpened: load()
    onSortChanged: if (visible)
        load()
    function openFile(e) {
        p.close();
        w.call("OpenDefault", { uri: e.uri });
    }

    component Thumb: Rectangle {
        property var entry: ({})
        property int box: 48
        width: box
        height: box * 7 / 8
        radius: 6
        color: Style.alpha(Style.text, 0.05)
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

    Column {
        width: p.kind === "fan" ? 240 : p.kind === "list" ? 320 : 340
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

        // Fan: the newest eight, newest at the bottom, curving.
        Column {
            visible: p.kind === "fan"
            width: parent.width
            spacing: 4
            Rectangle {
                width: parent.width
                height: 30
                radius: Style.radiusSmall
                color: openAll.containsMouse ? Style.rowHover : "transparent"
                Label {
                    anchors.centerIn: parent
                    text: "Open in Files · " + p.entries.length + " items"
                    role: "secondary"
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
                model: p.kind === "fan" ? p.entries.slice(0, 8).reverse() : []
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    readonly property int i: Math.min(8, p.entries.length) - 1 - index
                    x: 3 * i * i
                    width: parent.width - 3 * 49
                    height: 52
                    radius: Style.radiusSmall
                    color: fanArea.containsMouse ? Style.rowHover : "transparent"
                    Thumb {
                        x: 2
                        anchors.verticalCenter: parent.verticalCenter
                        entry: parent.modelData
                        box: 48
                    }
                    Label {
                        x: 58
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 62
                        text: parent.modelData.name
                    }
                    MouseArea {
                        id: fanArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: p.openFile(parent.modelData)
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
