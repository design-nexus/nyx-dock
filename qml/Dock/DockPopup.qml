import QtQuick
import Quickshell
import Nyx.Ui

// A popup from the dock, on the side away from the screen's edge. While it's open the
// dock stays up (a hold) and, if it asks, has the keyboard; Escape or a click outside
// closes it (no grab in test runs, NYX_SHELL_NO_KEYBOARD=1).
PopupWindow {
    id: p

    property var dockWindow
    property Item anchorTo
    property bool keyboard: false
    readonly property var w: dockWindow
    readonly property bool keyboardOff: Quickshell.env("NYX_SHELL_NO_KEYBOARD") === "1"
    default property alias content: body.data
    property alias body: body
    property int pad: 8

    signal popupOpened
    signal popupClosed

    function open() {
        if (visible)
            return;
        visible = true;
    }
    function close() {
        visible = false;
    }

    property bool held: false
    onVisibleChanged: {
        if (visible && !held) {
            held = true;
            w.hold(true);
            if (keyboard)
                w.wantsKeyboard = true;
            popupOpened();
        } else if (!visible && held) {
            held = false;
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
    grabFocus: !keyboardOff
    color: "transparent"
    visible: false
    implicitWidth: body.implicitWidth + 2 * pad
    implicitHeight: body.implicitHeight + 2 * pad

    Surface {
        anchors.fill: parent
        shadow: false
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
