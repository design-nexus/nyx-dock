import QtQuick
import QtQuick.Effects

// A picture that is all one neutral colour (an app's white or black tray icon, marked
// `mono` by the host) drawn in `color` instead, so it shows on any theme.
Item {
    id: t
    property url source
    property color color
    property real size: 16
    implicitWidth: size
    implicitHeight: size

    Image {
        id: img
        anchors.fill: parent
        source: t.source
        sourceSize: Qt.size(Math.max(1, t.size * 2), Math.max(1, t.size * 2))
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        visible: false
        layer.enabled: true
    }
    Rectangle {
        id: ink
        anchors.fill: parent
        color: t.color
        visible: false
        layer.enabled: true
    }
    MultiEffect {
        anchors.fill: parent
        source: ink
        maskEnabled: true
        maskSource: img
    }
}
