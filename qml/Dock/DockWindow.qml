import QtQuick
import Quickshell
import Quickshell.Wayland
import Nyx.Ui

// One screen's dock: the bar in a layer surface at its edge, the thin strip that
// brings it back, and its popups. Input reaches only the bar (none while hidden).
Scope {
    id: w

    required property var client
    required property string name
    property var state_: ({})
    readonly property var mon: (state_.monitors || []).find(m => m.name === name) || ({ items: [], settings: ({}) })
    readonly property var global: state_.global || ({})
    readonly property var settings: mon.settings || ({ icon_size: 48, position: "bottom" })
    readonly property string position: settings.position || "bottom"
    readonly property bool horizontal: position === "bottom" || position === "top"
    readonly property int iconSize: settings.icon_size || 48
    readonly property var screen: Quickshell.screens.find(s => s.name === name) || null
    readonly property bool keyboardOff: Quickshell.env("NYX_SHELL_NO_KEYBOARD") === "1"
    // How far out of sight the bar is (0 up, 1 away). During a workspace change it
    // follows the backend's offset; after one it goes on from wherever that left it, so
    // the end of a change never starts the hide or show over from the old state.
    property real slide: 1
    readonly property real shownSlide: slide
    property bool primed: false
    function follow() {
        if (!state_.monitors)
            return;
        const offset = mon.offset;
        if (offset !== null && offset !== undefined) {
            slideAnim.stop();
            slide = offset;
            return;
        }
        const to = mon.hidden ? 1 : 0;
        if (!primed) {
            primed = true;
            slide = to;
            return;
        }
        if (slideAnim.running ? slideAnim.to === to : slide === to)
            return;
        slideAnim.stop();
        slideAnim.from = slide;
        slideAnim.to = to;
        slideAnim.duration = Style.reducedMotion ? 0 : (mon.slide_ms || 180);
        slideAnim.easing.type = mon.hidden ? Easing.InQuad : Easing.OutCubic;
        slideAnim.start();
    }
    onMonChanged: follow()
    Component.onCompleted: follow()
    NumberAnimation {
        id: slideAnim
        target: w
        property: "slide"
    }
    property int holds: 0
    property bool hovered: false
    // An open popup keeps the keyboard (search in the Apps panel, the card's keys).
    property bool wantsKeyboard: false

    function openMenu() {
        bar.openMenu();
    }
    function call(op, body, cb) {
        client.request(op, Object.assign({ monitor: name }, body || {}), cb || null);
    }
    function hold(on) {
        holds += on ? 1 : -1;
        call("Hold", { on: on });
    }
    onHoveredChanged: call("Hover", { on: hovered })

    Connections {
        target: w.client
        function onEvent(ev, body) {
            if (ev === "PointerLeft" && body.monitor === w.name)
                w.hovered = false;
        }
        function onReconnected() {
            w.reportGeometry();
        }
    }

    // The bar's place on its monitor when up, for the backend's overlap test.
    function reportGeometry() {
        if (!win.screen || bar.width <= 0)
            return;
        const sw = win.screen.width, sh = win.screen.height;
        let wx = 0, wy = 0;
        if (position === "bottom") {
            wx = (sw - win.width) / 2;
            wy = sh - win.height;
        } else if (position === "top") {
            wx = (sw - win.width) / 2;
        } else if (position === "left") {
            wy = (sh - win.height) / 2;
        } else {
            wx = sw - win.width;
            wy = (sh - win.height) / 2;
        }
        call("Geometry", { x: wx + bar.homeX, y: wy + bar.homeY, w: bar.width, h: bar.height });
    }

    PanelWindow {
        id: win
        screen: w.screen
        // Mapped even while hidden (the bar out of sight, no input): a swipe away from a
        // fullscreen workspace shows only the top-layer surfaces mapped when it starts, and
        // one mapped during it stays transparent until it ends.
        visible: w.screen !== null
        color: "transparent"
        anchors.bottom: w.position === "bottom"
        anchors.top: w.position === "top"
        anchors.left: w.position === "left"
        anchors.right: w.position === "right"
        // Margins as the GTK dock's bar had them (16 from the screen's middle side,
        // 10 from the edge, 22 along it), plus room for the shadow.
        implicitWidth: bar.width + (w.horizontal ? 44 : 26) + 16
        implicitHeight: bar.height + (w.horizontal ? 26 : 44) + 16
        exclusionMode: w.mon.reserve ? ExclusionMode.Normal : ExclusionMode.Ignore
        exclusiveZone: w.mon.reserve ? (w.horizontal ? bar.height + 10 : bar.width + 10) : 0
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "nyx-dock"
        WlrLayershell.keyboardFocus: (w.wantsKeyboard && !w.keyboardOff) ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
        mask: Region {
            item: bar
        }

        DockBar {
            id: bar
            dockWindow: w
            // Up: 10 px from the screen's edge, centred along it.
            readonly property real homeX: w.horizontal ? (win.width - width) / 2 : (w.position === "left" ? 18 : win.width - width - 18)
            readonly property real homeY: w.horizontal ? (w.position === "bottom" ? win.height - height - 10 : 10) : (win.height - height) / 2
            readonly property real away: (w.horizontal ? height : width) + 24
            x: homeX + (w.position === "left" ? -away : w.position === "right" ? away : 0) * w.shownSlide
            y: homeY + (w.position === "bottom" ? away : w.position === "top" ? -away : 0) * w.shownSlide
            opacity: Math.min((1 - w.shownSlide) / 0.4, 1)
            onWidthChanged: Qt.callLater(w.reportGeometry)
            onHeightChanged: Qt.callLater(w.reportGeometry)
        }
        onWidthChanged: Qt.callLater(w.reportGeometry)
        onHeightChanged: Qt.callLater(w.reportGeometry)
    }

    // The strip at the screen's edge that brings a hidden dock back.
    PanelWindow {
        screen: w.screen
        visible: w.screen !== null
        color: "transparent"
        anchors.bottom: w.position === "bottom"
        anchors.top: w.position === "top"
        anchors.left: w.position === "left"
        anchors.right: w.position === "right"
        implicitWidth: w.horizontal ? Math.max(96, bar.width) : 2
        implicitHeight: w.horizontal ? 2 : Math.max(96, bar.height)
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "nyx-dock-edge"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            onEntered: w.call("Edge", {})
        }
    }
}
