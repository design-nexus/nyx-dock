import QtQuick
import Quickshell

// A tooltip for its parent item, in a popup so it can leave the surface (a dock's
// icons): shown after a short rest, above the item unless `below`.
PopupWindow {
    id: tip

    property string text: ""
    property bool shown: false
    property bool below: false
    property Item target: parent
    property int delay: 500

    anchor.item: target
    anchor.rect.x: target ? target.width / 2 : 0
    anchor.rect.y: below ? (target ? target.height + 6 : 0) : -6
    anchor.edges: below ? Edges.Bottom : Edges.Top
    anchor.gravity: below ? Edges.Bottom : Edges.Top
    grabFocus: false
    color: "transparent"
    implicitWidth: label.implicitWidth + 16
    implicitHeight: label.implicitHeight + 10
    visible: false

    onShownChanged: {
        if (shown && text !== "")
            wait.restart();
        else {
            wait.stop();
            visible = false;
        }
    }

    Timer {
        id: wait
        interval: tip.delay
        onTriggered: tip.visible = tip.shown && tip.text !== "" && tip.target !== null && tip.target.visible
    }

    Rectangle {
        anchors.fill: parent
        radius: Style.radiusSmall
        color: Style.popover
        border.width: 1
        border.color: Style.alpha(Style.border, 0.6)

        Label {
            id: label
            anchors.centerIn: parent
            text: tip.text
            role: "secondary"
        }
    }
}
