import QtQuick
import Quickshell
import Nyx.Ui

// A popup from the dock, on the side away from the screen's edge. While it's open the
// dock stays up (a hold) and, if it asks, has the keyboard; Escape or a click outside
// closes it (no grab in test runs, NYX_SHELL_NO_KEYBOARD=1).
//
// It opens once what it shows is there: `preparing` asks the popup to load (it sets
// `waiting` and calls `ready()` when done), and the surface maps a frame later, at its
// final size. A popup shown first and grown after is resized on screen, which the
// compositor shows as a flicker. Then the content fades and slides in.
PopupWindow {
    id: p

    property var dockWindow
    property Item anchorTo
    property bool keyboard: false
    // No panel: the content draws over the desktop (the folder fan).
    property bool bare: false
    // Takes the pointer and keyboard while open (a click outside closes it). A popup
    // that opens on hover must not: the grab takes the pointer off the dock, the dock
    // hears it leave, and the popup closes and opens again over and over.
    property bool grab: true
    readonly property var w: dockWindow
    readonly property bool keyboardOff: Quickshell.env("NYX_SHELL_NO_KEYBOARD") === "1"
    default property alias content: body.data
    property alias body: body
    property int pad: 8
    // Loading what it shows; `ready()` when done.
    property bool waiting: false
    property bool pending: false
    property bool entered: false

    signal preparing
    signal popupOpened
    signal popupClosed

    function open() {
        if (visible || pending)
            return;
        pending = true;
        waiting = false;
        preparing();
        settle.restart();
    }
    function ready() {
        waiting = false;
        if (pending)
            settle.restart();
    }
    function close() {
        pending = false;
        visible = false;
    }

    // One frame for the layout to settle once nothing is loading; at most 400 ms.
    Timer {
        id: settle
        interval: 16
        onTriggered: {
            if (!p.pending || p.waiting)
                return;
            p.pending = false;
            p.entered = false;
            p.visible = true;
        }
    }
    Timer {
        interval: 400
        running: p.pending
        onTriggered: p.ready()
    }

    property bool held: false
    onVisibleChanged: {
        if (visible && !held) {
            held = true;
            w.hold(true);
            if (keyboard)
                w.wantsKeyboard = true;
            popupOpened();
            Qt.callLater(() => p.entered = true);
        } else if (!visible && held) {
            held = false;
            entered = false;
            w.hold(false);
            if (keyboard)
                w.wantsKeyboard = false;
            popupClosed();
        }
    }
    Component.onDestruction: if (held)
        w.hold(false)

    anchor.item: anchorTo
    anchor.rect.x: !anchorTo ? 0 : w.position === "left" ? anchorTo.width + 10 : w.position === "right" ? -10 : anchorTo.width / 2
    anchor.rect.y: !anchorTo ? 0 : w.position === "bottom" ? -10 : w.position === "top" ? anchorTo.height + 10 : anchorTo.height / 2
    anchor.edges: w.position === "bottom" ? Edges.Top : w.position === "top" ? Edges.Bottom : w.position === "left" ? Edges.Right : Edges.Left
    anchor.gravity: w.position === "bottom" ? Edges.Top : w.position === "top" ? Edges.Bottom : w.position === "left" ? Edges.Right : Edges.Left
    anchor.adjustment: PopupAdjustment.All
    grabFocus: grab && !keyboardOff
    color: "transparent"
    visible: false
    implicitWidth: Math.max(1, body.implicitWidth + 2 * pad)
    implicitHeight: Math.max(1, body.implicitHeight + 2 * pad)

    Item {
        id: frame
        width: parent.width
        height: parent.height
        opacity: p.entered ? 1 : 0
        // Slides in from the dock's side.
        property real shift: p.entered ? 0 : 8
        transform: Translate {
            x: p.w.position === "left" ? -frame.shift : p.w.position === "right" ? frame.shift : 0
            y: p.w.position === "bottom" ? frame.shift : p.w.position === "top" ? -frame.shift : 0
        }
        Behavior on opacity {
            NumberAnimation {
                duration: Style.reducedMotion ? 0 : 140
                easing.type: Easing.OutCubic
            }
        }
        Behavior on shift {
            NumberAnimation {
                duration: Style.reducedMotion ? 0 : 140
                easing.type: Easing.OutCubic
            }
        }

        Surface {
            anchors.fill: parent
            visible: !p.bare
            shadow: false
        }
        FocusScope {
            id: body
            x: p.pad
            y: p.pad
            width: parent.width - 2 * p.pad
            height: parent.height - 2 * p.pad
            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
            focus: true
            Keys.onEscapePressed: p.close()
        }
    }
}
