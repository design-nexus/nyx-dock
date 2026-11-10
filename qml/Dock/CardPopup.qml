import QtQuick
import Nyx.Ui

// An app's card: what it is, its player, the list that fits it (Calendar's next
// events, a file manager's folders, a browser's bookmarks, Music's library, the files
// it opened lately), its windows, its own actions, and Keep in Dock, Show windows and
// Quit.
DockPopup {
    id: p

    property var tile
    readonly property var item: tile.item
    property var info: null
    property var list: null
    property var player: null

    anchorTo: tile
    keyboard: true
    pad: 12

    // Everything the card shows arrives before it opens, so it opens at its size.
    property int loads: 0
    function loaded() {
        loads -= 1;
        if (loads <= 0)
            ready();
    }
    onPreparing: {
        info = null;
        list = null;
        player = null;
        const running = (item.windows || []).length > 0;
        loads = running ? 3 : 2;
        waiting = true;
        w.client.request("Card", { key: item.key }, ok => {
            p.info = ok;
            p.loaded();
        });
        w.client.request("CardList", { key: item.key }, ok => {
            p.list = ok;
            p.loaded();
        });
        if (running)
            w.client.request("Player", { key: item.key }, ok => {
                p.player = ok;
                p.loaded();
            });
    }
    function readPlayer() {
        if ((item.windows || []).length > 0)
            w.client.request("Player", { key: item.key }, ok => p.player = ok);
    }
    Timer {
        interval: 1000
        repeat: true
        running: p.visible && p.player !== null
        onTriggered: p.readPlayer()
    }
    function clock(us) {
        const s = Math.max(0, Math.floor(us / 1000000));
        return Math.floor(s / 60) + ":" + (s % 60 < 10 ? "0" : "") + (s % 60);
    }
    function act(f) {
        p.close();
        f();
    }

    Column {
        width: 300
        spacing: 10
        Row {
            spacing: 10
            Icon {
                size: 36
                name: p.info ? p.info.icon : p.item.icon
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                Label {
                    width: 240
                    text: p.info ? p.info.name : p.item.name
                    font.weight: Font.DemiBold
                }
                Label {
                    readonly property int n: (p.item.windows || []).length
                    readonly property string status: n === 0 ? "Not running" : n === 1 ? "1 window" : n + " windows"
                    text: p.info && p.info.generic ? p.info.generic + " · " + status : status
                    role: "secondary"
                    dim: true
                }
            }
        }

        // What's playing, with its controls.
        Rectangle {
            visible: p.player !== null && p.player !== undefined
            width: parent.width
            height: 76
            radius: Style.radiusCard
            color: Style.alpha(Style.text, 0.05)
            Row {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 10
                Rectangle {
                    width: 60
                    height: 60
                    radius: Style.radiusSmall
                    color: Style.alpha(Style.text, 0.06)
                    clip: true
                    Image {
                        anchors.fill: parent
                        visible: !!(p.player && p.player.art)
                        source: p.player && p.player.art ? p.player.art : ""
                        fillMode: Image.PreserveAspectCrop
                    }
                    Icon {
                        anchors.centerIn: parent
                        visible: !(p.player && p.player.art)
                        name: "audio-x-generic-symbolic"
                        size: 24
                        recolor: true
                    }
                }
                Column {
                    width: parent.width - 70 - 110
                    anchors.verticalCenter: parent.verticalCenter
                    Label {
                        width: parent.width
                        text: p.player && p.player.title ? p.player.title : "Nothing playing"
                        font.weight: Font.Medium
                    }
                    Label {
                        width: parent.width
                        text: p.player ? p.player.artist : ""
                        role: "secondary"
                        dim: true
                    }
                    Label {
                        visible: !!(p.player && p.player.length_us > 0)
                        text: p.player ? p.clock(p.player.position_us) + " / " + p.clock(p.player.length_us) : ""
                        role: "caption"
                        dim: true
                    }
                }
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    IconButton {
                        icon: "media-skip-backward-symbolic"
                        enabled: !!(p.player && p.player.can_previous)
                        onClicked: p.w.client.request("PlayerCommand", { key: p.item.key, method: "Previous" }, () => p.readPlayer())
                    }
                    IconButton {
                        icon: p.player && p.player.playing ? "media-playback-pause-symbolic" : "media-playback-start-symbolic"
                        onClicked: p.w.client.request("PlayerCommand", { key: p.item.key, method: "PlayPause" }, () => p.readPlayer())
                    }
                    IconButton {
                        icon: "media-skip-forward-symbolic"
                        enabled: !!(p.player && p.player.can_next)
                        onClicked: p.w.client.request("PlayerCommand", { key: p.item.key, method: "Next" }, () => p.readPlayer())
                    }
                }
            }
        }

        // The app's list.
        Column {
            width: parent.width
            visible: !!(p.list && (p.list.rows || []).length > 0 || (p.list && p.list.note))
            Label {
                text: p.list && p.list.title ? p.list.title : ""
                role: "group"
                color: Style.tertiary
                bottomPadding: 4
            }
            Row {
                visible: !!(p.list && p.list.chips)
                spacing: 6
                bottomPadding: 4
                Repeater {
                    model: p.list && p.list.chips ? p.list.chips : []
                    delegate: Button {
                        required property var modelData
                        text: modelData[0]
                        onClicked: p.act(() => p.w.call(p.info.calendar_app ? "Command" : "Launch", p.info.calendar_app ? { args: ["nyx-calendar", "--section", modelData[1]] } : { key: p.item.key }))
                    }
                }
            }
            Label {
                visible: !!(p.list && p.list.note)
                text: p.list && p.list.note ? p.list.note : ""
                role: "secondary"
                dim: true
            }
            Repeater {
                model: p.list ? p.list.rows || [] : []
                delegate: ListRow {
                    required property var modelData
                    width: parent.width
                    icon: modelData.icon
                    title: modelData.title
                    subtitle: modelData.sub || ""
                    onClicked: p.act(() => {
                        const r = modelData;
                        if (r.calendar_day)
                            p.w.call(p.info.calendar_app ? "Command" : "Launch", p.info.calendar_app ? { args: ["nyx-calendar", "--section", "day", "--date", r.calendar_day] } : { key: p.item.key });
                        else if (r.music_page)
                            p.w.call("Command", { args: ["nyx-music", "--section", r.music_page] });
                        else if (r.open)
                            p.w.call("OpenWith", { key: p.item.key, uris: [r.open] });
                    })
                }
            }
        }

        // Its windows, with more than one.
        Column {
            width: parent.width
            visible: (p.item.windows || []).length > 1
            Label {
                text: "Windows"
                role: "group"
                color: Style.tertiary
                bottomPadding: 4
            }
            Repeater {
                model: p.item.windows || []
                delegate: ListRow {
                    required property var modelData
                    width: parent.width
                    icon: p.item.icon
                    iconRecolor: false
                    title: modelData.title
                    selected: modelData.focused
                    onClicked: p.act(() => p.w.call("FocusWindow", { address: modelData.address }))
                }
            }
        }

        // Open / New window, and the app's own actions, three to a line.
        Flow {
            width: parent.width
            spacing: 6
            visible: !!p.info && p.info.is_app
            Button {
                readonly property bool running: (p.item.windows || []).length > 0
                visible: !!p.info && !(running && p.info.own_new_window)
                text: running ? "New window" : "Open"
                onClicked: p.act(() => p.w.call(running ? "NewInstance" : "Launch", { key: p.item.key }))
            }
            Repeater {
                model: p.info ? p.info.actions : []
                delegate: Button {
                    required property var modelData
                    text: modelData.name
                    onClicked: p.act(() => p.w.call("Launch", { key: p.item.key, action: modelData.id }))
                }
            }
        }

        Separator {
            width: parent.width
        }
        Row {
            width: parent.width
            spacing: 4
            Button {
                visible: !!p.item.app
                flat: true
                text: p.item.pinned ? "Remove from Dock" : "Keep in Dock"
                onClicked: p.act(() => p.w.call("Pin", { key: p.item.key, pinned: !p.item.pinned }))
            }
            Item {
                width: parent.width - 150 - quit.width - showWin.width
                height: 1
            }
            IconButton {
                id: showWin
                visible: (p.item.windows || []).length > 0 && p.w.global.switcher
                icon: "view-grid-symbolic"
                tooltip: "Show windows"
                onClicked: p.act(() => p.w.call("ShowWindows", { key: p.item.key }))
            }
            Button {
                id: quit
                visible: (p.item.windows || []).length > 0
                flat: true
                danger: true
                confirm: false
                text: (p.item.windows || []).length > 1 ? "Close all" : "Quit"
                onClicked: p.act(() => p.w.call("CloseApp", { key: p.item.key }))
            }
        }
    }
}
