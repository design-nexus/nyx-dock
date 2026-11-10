import QtQuick
import Nyx.Ui

// Every app from the app menu in a grid: type to search (Enter opens the first
// match), click to open, right-click to keep it in the dock or uninstall it. The search
// bar shows as you start typing, or always (`apps_search_bar`); the panel keeps its size
// either way, the grid giving the bar its room.
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
    readonly property bool barAlways: w.global.apps_search_bar === true
    readonly property bool searching: barAlways || query !== ""
    readonly property int barRoom: 46
    readonly property int columns: names ? 7 : 9

    anchorTo: tile
    keyboard: true
    pad: 12

    // The list from last time shows at once and is read again behind it; the first
    // opening waits for it.
    onPreparing: {
        query = "";
        search.clear();
        waiting = apps.length === 0;
        w.client.request("Apps", {}, ok => {
            p.apps = ok || [];
            p.ready();
        });
    }
    onPopupOpened: searching ? search.focusInput() : keys.forceActiveFocus()
    onSearchingChanged: if (!searching && visible)
        keys.forceActiveFocus()
    readonly property int cell: names ? 84 : 64
    // As tall as every app needs (searching doesn't resize it), up to 60 % of the screen.
    readonly property real listHeight: Math.min(Math.ceil(apps.length / columns) * cell, Math.max(200, Math.min(520, (w.screen ? w.screen.height : 800) * 0.6)))
    function launch(a) {
        p.close();
        w.call("Launch", { key: a.id });
    }
    function pinned(id) {
        return (w.mon.items || []).some(i => i.key === id && i.pinned);
    }

    // Typing while the bar is hidden: the first key opens it with that letter.
    Item {
        id: keys
        width: 1
        height: 1
        Keys.onPressed: e => {
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
                if (p.shown.length)
                    p.launch(p.shown[0]);
            } else if (e.text !== "" && e.text.trim() !== "" && !(e.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                search.text = e.text;
                search.focusInput();
                search.input.cursorPosition = search.text.length;
            } else
                return;
            e.accepted = true;
        }
    }

    Column {
        width: p.columns * (p.names ? 84 : 64)
        Item {
            width: parent.width
            height: p.searching ? p.barRoom : 0
            clip: true
            Behavior on height {
                NumberAnimation {
                    duration: Style.reducedMotion ? 0 : 120
                    easing.type: Easing.OutCubic
                }
            }
            SearchField {
                id: search
                width: parent.width
                placeholder: "Search apps"
                opacity: p.searching ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: Style.reducedMotion ? 0 : 120
                    }
                }
                onTextChanged: p.query = text
                onAccepted: if (p.shown.length)
                    p.launch(p.shown[0])
                onEscaped: p.close()
            }
        }

        Flickable {
            width: parent.width
            // The bar's room comes out of the grid's, so the panel never changes size.
            height: p.listHeight + p.barRoom - parent.children[0].height
            clip: true
            contentHeight: grid.height
            boundsBehavior: Flickable.StopAtBounds
            Label {
                visible: p.shown.length === 0 && p.apps.length > 0
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                topPadding: 20
                text: "No apps found"
                dim: true
            }
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
