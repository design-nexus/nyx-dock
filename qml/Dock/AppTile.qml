import QtQuick
import QtQuick.Effects
import Quickshell
import Nyx.Ui

// One app on the dock: its icon, a mark per window (up to three; an accent bar for the
// app you're in), its badge and progress. Click: open it or bring its window forward
// (again: the next window); middle-click: a new window; right-click: its card; rest
// the pointer: its windows' previews. Drag it along the dock to pin it there, or off
// to unpin.
Item {
    id: t

    property var item: ({})
    property int index_: 0
    property var dockWindow
    property var barItem
    readonly property var w: dockWindow
    readonly property int size: w.iconSize
    readonly property int windows: (item.windows || []).length
    readonly property bool hovered: area.containsMouse
    // The first running app after the pins: a faint line before it.
    property bool leadLine: false
    readonly property int lead: leadLine ? 7 : 0
    readonly property bool horizontal: w.horizontal

    width: size + 8 + (horizontal ? lead : 0)
    height: size + 12 + (horizontal ? 0 : lead)

    // From the keyboard (Super+Ctrl+Alt+D focuses the dock): Enter opens it or brings its
    // window forward, the Menu key opens its card.
    function activate() {
        preview.close();
        w.keyboardMode = false;
        w.call("Activate", { key: item.key });
    }

    // How the icon answers a click (`click_animation`). Squish and Depress follow the
    // button; the others play once it's clicked. Keys play the whole press.
    readonly property string animation: Style.reducedMotion ? "none" : (w.global.click_animation || "squish")
    readonly property bool held: area.pressed && (area.pressedButtons & Qt.LeftButton) && !area.drag.active
    // Bounce on an app with no window: hops until one opens, or 5 s.
    property bool launching: false
    property real dim: 0
    // Bounce and Depress move the icon away from the screen's edge (or into it).
    property real hop: 0
    onHeldChanged: {
        if (animation === "squish") {
            (held ? squishUp : squishDown).stop();
            (held ? squishDown : squishUp).start();
        } else if (animation === "depress") {
            (held ? depressUp : depressDown).stop();
            (held ? depressDown : depressUp).start();
        }
    }
    onWindowsChanged: if (windows > 0)
        launching = false
    function play(fromKeys) {
        switch (animation) {
        case "squish":
            if (fromKeys)
                squishTap.restart();
            break;
        case "depress":
            if (fromKeys)
                depressTap.restart();
            break;
        case "bounce":
            if (windows === 0 && item.app) {
                launching = true;
                launchLimit.restart();
                if (!launchHop.running)
                    launchHop.start();
            } else if (!launchHop.running) {
                bounce.restart();
            }
            break;
        case "pop":
            pop.restart();
            break;
        case "ripple":
            ripple.restart();
            break;
        case "glow":
            glow.restart();
            break;
        }
    }
    Timer {
        id: launchLimit
        interval: 5000
        onTriggered: t.launching = false
    }

    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: item.name || ""
    Accessible.description: windows === 0 ? "" : windows === 1 ? "1 window" : windows + " windows"
    Accessible.onPressAction: {
        play(true);
        activate();
    }
    Keys.onPressed: e => {
        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) {
            play(true);
            activate();
            e.accepted = true;
        } else if (e.key === Qt.Key_Menu || (e.key === Qt.Key_F10 && (e.modifiers & Qt.ShiftModifier))) {
            card.open();
            e.accepted = true;
        }
    }

    Rectangle {
        visible: t.leadLine
        x: t.horizontal ? 1 : 18
        y: t.horizontal ? 18 : 1
        width: t.horizontal ? 1 : t.size + 8 - 40
        height: t.horizontal ? t.height - 40 : 1
        color: Style.alpha(Style.text, 0.08)
    }
    Item {
        id: tile
        x: t.horizontal ? t.lead : 0
        y: t.horizontal ? 0 : t.lead
        width: t.size + 8
        height: t.size + 12

        FocusRing {
            target: t
            anchors.fill: tile
            radius_: 14
        }
        Rectangle {
            anchors.fill: parent
            radius: 12
            color: drop.containsDrag ? Style.alpha(Style.accent, 0.18) : t.hovered ? Style.alpha(Style.text, 0.07) : "transparent"
            border.width: drop.containsDrag ? 2 : 0
            border.color: Style.accent
            Behavior on color {
                ColorAnimation {
                    duration: Style.fast
                }
            }
        }
        // Asking for attention: a warm glow behind the icon.
        MultiEffect {
            anchors.fill: icon
            source: icon
            visible: !!t.item.urgent
            shadowEnabled: true
            shadowColor: Style.alpha(Style.warning, 0.55)
            shadowBlur: 0.6
            blurMax: 16
            shadowVerticalOffset: 0
            shadowHorizontalOffset: 0
        }
        // Click: Glow, a brief accent glow behind the icon.
        MultiEffect {
            anchors.fill: icon
            source: icon
            visible: opacity > 0
            opacity: 0
            shadowEnabled: true
            shadowColor: Style.accent
            shadowBlur: 0.8
            blurMax: 24
            shadowScale: 1.06
            shadowVerticalOffset: 0
            shadowHorizontalOffset: 0
            SequentialAnimation on opacity {
                id: glow
                running: false
                NumberAnimation {
                    to: 1
                    duration: 120
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    to: 0
                    duration: 380
                    easing.type: Easing.InOutQuad
                }
            }
        }
        // Click: Ripple, an accent ring in the icon's shape spreading out.
        Rectangle {
            id: ring
            x: icon.x
            y: icon.y
            width: t.size
            height: t.size
            radius: t.size * 0.26
            color: "transparent"
            border.color: Style.accent
            border.width: 3
            opacity: 0
            visible: opacity > 0
            ParallelAnimation {
                id: ripple
                NumberAnimation {
                    target: ring
                    property: "scale"
                    from: 1
                    to: 1.6
                    duration: 450
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    target: ring
                    property: "border.width"
                    from: 3
                    to: 1
                    duration: 450
                }
                SequentialAnimation {
                    NumberAnimation {
                        target: ring
                        property: "opacity"
                        from: 1
                        to: 0.9
                        duration: 200
                    }
                    NumberAnimation {
                        target: ring
                        property: "opacity"
                        to: 0
                        duration: 250
                        easing.type: Easing.OutCubic
                    }
                }
            }
        }
        Icon {
            id: icon
            x: 4
            y: t.hovered && !area.drag.active ? 2 : 4
            Behavior on y {
                NumberAnimation {
                    duration: Style.fast
                }
            }
            size: t.size
            name: t.item.icon || "application-x-executable"
            recolor: false
            opacity: area.drag.active ? 0.3 : 1
            transform: Translate {
                x: t.w.position === "left" ? t.hop : t.w.position === "right" ? -t.hop : 0
                y: t.w.position === "bottom" ? -t.hop : t.w.position === "top" ? t.hop : 0
            }
            // Depress darkens it while held: its silhouette in black over it.
            Icon {
                anchors.fill: parent
                size: t.size
                name: icon.name
                recolor: true
                color: "black"
                opacity: t.dim
                visible: opacity > 0
            }
        }
        // Squish: smaller while held, springs back past full size on release.
        NumberAnimation {
            id: squishDown
            target: icon
            property: "scale"
            to: 0.85
            duration: 110
            easing.type: Easing.OutCubic
        }
        SequentialAnimation {
            id: squishUp
            NumberAnimation {
                target: icon
                property: "scale"
                to: 1.06
                duration: 140
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: icon
                property: "scale"
                to: 0.98
                duration: 120
                easing.type: Easing.InOutQuad
            }
            NumberAnimation {
                target: icon
                property: "scale"
                to: 1
                duration: 120
                easing.type: Easing.InOutQuad
            }
        }
        SequentialAnimation {
            id: squishTap
            ScriptAction {
                script: squishDown.start()
            }
            PauseAnimation {
                duration: 260
            }
            ScriptAction {
                script: squishUp.start()
            }
        }
        // Depress: sinks and darkens while held, like a key.
        ParallelAnimation {
            id: depressDown
            NumberAnimation {
                target: t
                property: "hop"
                to: -3
                duration: 80
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: t
                property: "dim"
                to: 0.2
                duration: 80
                easing.type: Easing.OutCubic
            }
        }
        ParallelAnimation {
            id: depressUp
            NumberAnimation {
                target: t
                property: "hop"
                to: 0
                duration: 160
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: t
                property: "dim"
                to: 0
                duration: 160
                easing.type: Easing.OutCubic
            }
        }
        SequentialAnimation {
            id: depressTap
            ScriptAction {
                script: depressDown.start()
            }
            PauseAnimation {
                duration: 230
            }
            ScriptAction {
                script: depressUp.start()
            }
        }
        // Pop: a little larger, then back.
        SequentialAnimation {
            id: pop
            NumberAnimation {
                target: icon
                property: "scale"
                to: 1.15
                duration: 90
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: icon
                property: "scale"
                to: 1
                duration: 170
                easing.type: Easing.InOutQuad
            }
        }
        // Bounce: two hops, the second lower.
        SequentialAnimation {
            id: bounce
            NumberAnimation {
                target: t
                property: "hop"
                to: 14
                duration: 200
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: t
                property: "hop"
                to: 0
                duration: 180
                easing.type: Easing.InCubic
            }
            NumberAnimation {
                target: t
                property: "hop"
                to: 5
                duration: 100
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: t
                property: "hop"
                to: 0
                duration: 140
                easing.type: Easing.InCubic
            }
        }
        // Launching: one hop at a time while the app has no window.
        SequentialAnimation {
            id: launchHop
            NumberAnimation {
                target: t
                property: "hop"
                to: 14
                duration: 200
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: t
                property: "hop"
                to: 0
                duration: 180
                easing.type: Easing.InCubic
            }
            PauseAnimation {
                duration: 140
            }
            onFinished: if (t.launching)
                Qt.callLater(launchHop.start)
        }
        Rectangle {
            visible: t.item.badge !== null && t.item.badge !== undefined
            anchors.right: parent.right
            anchors.top: parent.top
            height: 18
            width: Math.max(18, badgeText.implicitWidth + 10)
            radius: 9
            color: Style.danger
            border.width: 2
            border.color: Style.panel
            Label {
                id: badgeText
                anchors.centerIn: parent
                text: t.item.badge > 99 ? "99+" : String(t.item.badge)
                color: "white"
                size: 11
                font.weight: Font.Bold
            }
        }
        // Progress: an accent bar on a dark track across the icon's foot.
        Rectangle {
            visible: t.item.progress !== null && t.item.progress !== undefined
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            anchors.bottomMargin: 11
            height: 5
            radius: 3
            color: Qt.rgba(0, 0, 0, 0.55)
            border.width: 1
            border.color: Qt.rgba(1, 1, 1, 0.15)
            clip: true
            Rectangle {
                width: parent.width * Math.max(0, Math.min(1, t.item.progress || 0))
                height: parent.height
                radius: 3
                color: Style.accent
            }
        }
        // A mark per window, up to three; the app you're in gets one accent bar.
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 2
            spacing: 3
            Repeater {
                model: t.item.focused ? Math.min(1, t.windows) : Math.min(3, t.windows)
                Rectangle {
                    width: t.item.focused ? 16 : 4
                    height: 4
                    radius: 2
                    color: t.item.urgent ? Style.warning : t.item.focused ? Style.accent : Style.alpha(Style.text, 0.55)
                }
            }
        }
    }

    Tip {
        target: t
        text: t.item.name || ""
        shown: t.hovered && !(t.w.global.previews && t.windows > 0) && !area.drag.active
        below: t.w.position === "top"
    }

    Timer {
        id: previewWait
        interval: preview.visible ? 0 : (t.w.global.preview_delay_ms || 350)
        onTriggered: if (t.hovered && t.windows > 0 && t.w.global.previews !== false && !card.visible)
            preview.open()
    }
    onHoveredChanged: {
        if (hovered)
            previewWait.restart();
        else {
            previewWait.stop();
            preview.leaveSoon();
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        drag.target: t.item.app ? ghost : null
        drag.threshold: 8
        onPressed: m => {
            ghost.x = tile.x + icon.x;
            ghost.y = tile.y + icon.y;
        }
        onClicked: m => {
            if (m.button === Qt.RightButton) {
                preview.close();
                card.open();
            } else if (m.button === Qt.MiddleButton) {
                if (t.item.app)
                    t.w.call("NewInstance", { key: t.item.key });
            } else {
                t.play(false);
                t.activate();
            }
        }
        onReleased: {
            if (!drag.active)
                return;
            const target = ghost.Drag.target;
            ghost.Drag.drop();
            // Let go off the dock: a pinned app leaves it.
            if (!target && t.item.pinned)
                t.w.call("Pin", { key: t.item.key, pinned: false });
            ghost.x = tile.x + icon.x;
            ghost.y = tile.y + icon.y;
        }
    }
    Item {
        id: ghost
        width: t.size
        height: t.size
        visible: area.drag.active
        Drag.active: area.drag.active
        Drag.keys: ["nyx-dock-app"]
        Drag.hotSpot.x: width / 2
        Drag.hotSpot.y: height / 2
        property string key: t.item.key
        Icon {
            anchors.fill: parent
            size: t.size
            name: t.item.icon || "application-x-executable"
        }
        // Only a real drag holds the dock: a new tile's ghost starts visible and hides,
        // and that release took another popup's hold (the Apps panel's).
        property bool holding: false
        onVisibleChanged: if (visible !== holding) {
            holding = visible;
            t.w.hold(visible);
        }
        Component.onDestruction: if (holding)
            t.w.hold(false)
    }
    // Another app dropped here takes this place among the pins; files open with this app.
    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["nyx-dock-app", "text/uri-list"]
        onDropped: d => {
            if (d.source && d.source.key !== undefined) {
                if (d.source.key !== t.item.key) {
                    const pins = (t.w.mon.items || []).filter(i => i.pinned);
                    const at = t.item.pinned ? pins.findIndex(i => i.key === t.item.key) : pins.length;
                    t.w.call("Pin", { key: d.source.key, pinned: true, at: Math.max(0, at) });
                }
                d.accept();
            } else if (d.hasUrls && t.item.app) {
                t.w.call("OpenWith", { key: t.item.key, uris: d.urls.map(u => u.toString()) });
                d.accept();
            }
        }
    }

    // Screenshot hooks, as the GTK dock's: NYX_DOCK_TEST_CARD=<app>, NYX_DOCK_TEST_PREVIEW=<app>.
    Timer {
        interval: 800
        running: true
        onTriggered: {
            if (Quickshell.env("NYX_DOCK_TEST_CARD") === t.item.key)
                card.open();
            else if (Quickshell.env("NYX_DOCK_TEST_PREVIEW") === t.item.key)
                preview.open();
        }
    }
    // NYX_DOCK_TEST_CLICK=<app>: its click animation every 1.5 s, without opening it.
    Timer {
        interval: 1500
        repeat: true
        running: Quickshell.env("NYX_DOCK_TEST_CLICK") === t.item.key
        onTriggered: t.play(true)
    }
    PreviewPopup {
        id: preview
        tile: t
        dockWindow: t.w
    }
    CardPopup {
        id: card
        tile: t
        dockWindow: t.w
    }
}
