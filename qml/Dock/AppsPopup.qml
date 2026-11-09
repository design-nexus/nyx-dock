import QtQuick
import Nyx.Ui

// Every app from the app menu in a grid: type to search (Enter opens the first
// match), click to open, right-click to keep it in the dock or uninstall it.
DockPopup {
    id: p

    property var tile
    property var apps: []
    property string query: ""
    readonly property var shown: {
        const words = query.toLowerCase().split(/\s+/).filter(x => x !== "");
        return apps.filter(a => words.every(x => a.words.indexOf(x) >= 0));
    }
    readonly property bool names: w.global.app_names !== false
    readonly property int columns: names ? 7 : 9

    anchorTo: tile
    keyboard: true
    pad: 12

    onPopupOpened: {
        query = "";
        search.clear();
        w.client.request("Apps", {}, ok => p.apps = ok || []);
        search.focusInput();
    }
    function launch(a) {
        p.close();
        w.call("Launch", { key: a.id });
    }
    function pinned(id) {
        return (w.mon.items || []).some(i => i.key === id && i.pinned);
    }

    Column {
        width: p.columns * (p.names ? 84 : 64)
        spacing: 10
        SearchField {
            id: search
            width: parent.width
            placeholder: "Search apps"
            onTextChanged: p.query = text
            onAccepted: if (p.shown.length)
                p.launch(p.shown[0])
            onEscaped: p.close()
        }
        Label {
            visible: p.shown.length === 0 && p.apps.length > 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            topPadding: 20
            bottomPadding: 20
            text: "No apps found"
            dim: true
        }
        Flickable {
            width: parent.width
            height: Math.min(contentHeight, Math.max(200, Math.min(520, (p.w.screen ? p.w.screen.height : 800) * 0.6)))
            clip: true
            contentHeight: grid.height
            boundsBehavior: Flickable.StopAtBounds
            Grid {
                id: grid
                columns: p.columns
                Repeater {
                    model: p.shown
                    delegate: Rectangle {
                        id: cell
                        required property var modelData
                        width: p.names ? 84 : 64
                        height: p.names ? 84 : 64
                        radius: Style.radiusSmall
                        color: cellArea.containsMouse ? Style.rowHover : "transparent"
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 8
                            size: 48
                            name: cell.modelData.icon
                        }
                        Label {
                            visible: p.names
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 60
                            width: parent.width - 4
                            horizontalAlignment: Text.AlignHCenter
                            text: cell.modelData.name
                            role: "caption"
                        }
                        MouseArea {
                            id: cellArea
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            onClicked: m => m.button === Qt.RightButton ? cellMenu.openAt(m.x, m.y) : p.launch(cell.modelData)
                        }
                        Menu {
                            id: cellMenu
                            anchorItem: cell
                            entries: [{ id: "open", label: "Open" }, { id: "pin", label: p.pinned(cell.modelData.id) ? "Remove from Dock" : "Keep in Dock" }].concat(p.w.global.store ? [{ separator: true }, { id: "uninstall", label: "Uninstall…", danger: true }] : [])
                            onTriggered: id => {
                                if (id === "open")
                                    p.launch(cell.modelData);
                                else if (id === "pin")
                                    p.w.call("Pin", { key: cell.modelData.id, pinned: !p.pinned(cell.modelData.id) });
                                else {
                                    p.close();
                                    p.w.call("Uninstall", { id: cell.modelData.id });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
