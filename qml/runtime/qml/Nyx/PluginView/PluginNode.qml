import QtQuick
import QtQuick.Layouts
import Quickshell.Widgets
import Nyx.Ui
import "markup.js" as Markup

// One node of a plugin view and, through Loaders, its children. `node` is the node as
// sent; patches change `base`; `p` is what is drawn (base plus the hover and pressed
// property sets while they apply). Children come from the node as sent: patches
// never change them.
Item {
    id: n

    property var ctx: null
    property var node: ({})
    property var base: node
    property bool hov: false
    property bool prs: false
    readonly property var p: {
        let out = base || {};
        if (hov && out.hover)
            out = Object.assign({}, out, out.hover);
        if (prs && out.pressed)
            out = Object.assign({}, out, out.pressed);
        return out;
    }
    readonly property string kind: (node && node.node) || "box"
    readonly property var kids: node ? (node.children || (node.child ? [node.child] : [])) : []
    readonly property var m: (p.margin && p.margin.length === 4) ? p.margin : [0, 0, 0, 0]
    readonly property var classes: p["class"] || []

    function has(c) {
        return classes.indexOf(c) >= 0;
    }
    readonly property color fg: {
        if (p.color)
            return Style.token(p.color);
        if (has("danger") || has("urgent"))
            return Style.danger;
        if (has("warning"))
            return Style.warning;
        if (has("success"))
            return Style.success;
        if (has("accent") || has("active"))
            return Style.accent;
        if (has("dim"))
            return Style.dimText;
        return Style.text;
    }

    function applySet(set) {
        const b = Object.assign({}, base);
        for (const k in set) {
            if (k === "icon" && kind === "icon")
                b.name = set[k];
            else
                b[k] = set[k];
        }
        base = b;
    }

    Component.onCompleted: if (ctx && node && node.id)
        ctx.register(node.id, n)
    Component.onDestruction: if (ctx && node && node.id)
        ctx.unregister(node.id, n)

    visible: p.visible !== false
    implicitWidth: (p.width > 0 ? p.width : (content.item ? content.item.implicitWidth : 0)) + m[1] + m[3]
    implicitHeight: Math.min(ctx && ctx.maxHeight > 0 ? ctx.maxHeight : 1e9, (p.height > 0 ? p.height : (content.item ? content.item.implicitHeight : 0)) + m[0] + m[2])

    Rectangle {
        anchors.fill: parent
        visible: !!n.p.background
        color: Style.token(n.p.background)
        radius: Style.radiusSmall
    }

    Loader {
        id: content
        x: n.m[3]
        y: n.m[0]
        width: n.width - n.m[1] - n.m[3]
        height: n.height - n.m[0] - n.m[2]
        sourceComponent: ({
                box: boxC, overlay: overlayC, stack: stackC, label: labelC, icon: iconC, image: imageC,
                badge: badgeC, progress: progressC, graph: graphC, spinner: spinnerC, vector: vectorC,
                frame: frameC, row: rowC, toggle: toggleC, slider: sliderC, button: buttonC, entry: entryC,
                separator: separatorC, scroll: scrollC
            })[n.kind] || frameC
    }

    HoverHandler {
        enabled: !!(n.base && (n.base.hover || n.base.pressed))
        onHoveredChanged: n.hov = hovered
    }
    PointHandler {
        enabled: !!(n.base && n.base.pressed)
        onActiveChanged: n.prs = active
    }

    Tip {
        target: n
        text: n.p.tooltip || ""
        shown: !!n.p.tooltip && tipHover.hovered
    }
    HoverHandler {
        id: tipHover
        enabled: !!n.p.tooltip
    }

    // Children ------------------------------------------------------------------

    component Child: Loader {
        required property var modelData
        property bool horizontal: true
        property string align: "center"
        Layout.fillWidth: horizontal ? !!modelData.expand : (align === "fill" && !(modelData.width > 0))
        Layout.fillHeight: horizontal ? (align === "fill" && !(modelData.height > 0)) : !!modelData.expand
        Layout.alignment: horizontal ? ({ start: Qt.AlignTop, end: Qt.AlignBottom })[align] || Qt.AlignVCenter : ({ start: Qt.AlignLeft, end: Qt.AlignRight })[align] || Qt.AlignHCenter
        Layout.preferredWidth: item ? item.implicitWidth : 0
        Layout.preferredHeight: item ? item.implicitHeight : 0
        Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData })
    }

    Component {
        id: boxC
        GridLayout {
            readonly property bool horizontal: n.p.orientation !== "vertical"
            flow: horizontal ? GridLayout.LeftToRight : GridLayout.TopToBottom
            rows: horizontal ? 1 : -1
            columns: horizontal ? -1 : 1
            rowSpacing: n.p.spacing !== undefined ? n.p.spacing : 4
            columnSpacing: rowSpacing
            Repeater {
                model: n.kids
                delegate: Child {
                    horizontal: parent.horizontal
                    align: n.p.align || "center"
                }
            }
        }
    }

    Component {
        id: overlayC
        Item {
            implicitWidth: Math.max(0, ...Array.from({ length: rep.count }, (_, i) => rep.itemAt(i) ? rep.itemAt(i).implicitWidth : 0))
            implicitHeight: Math.max(0, ...Array.from({ length: rep.count }, (_, i) => rep.itemAt(i) ? rep.itemAt(i).implicitHeight : 0))
            Repeater {
                id: rep
                model: n.kids
                delegate: Loader {
                    required property var modelData
                    anchors.centerIn: parent
                    width: modelData.expand || index === 0 ? parent.width : (item ? item.implicitWidth : 0)
                    height: modelData.expand || index === 0 ? parent.height : (item ? item.implicitHeight : 0)
                    required property int index
                    Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData })
                }
            }
        }
    }

    Component {
        id: stackC
        Item {
            id: stack
            readonly property string show: n.p.show || (n.kids.length ? (n.kids[0].id || "") : "")
            implicitWidth: Math.max(0, ...Array.from({ length: srep.count }, (_, i) => srep.itemAt(i) ? srep.itemAt(i).implicitWidth : 0))
            implicitHeight: Math.max(0, ...Array.from({ length: srep.count }, (_, i) => srep.itemAt(i) ? srep.itemAt(i).implicitHeight : 0))
            Repeater {
                id: srep
                model: n.kids
                delegate: Loader {
                    required property var modelData
                    required property int index
                    readonly property bool current: (modelData.id || "") === stack.show || (stack.show === "" && index === 0)
                    anchors.fill: parent
                    opacity: current ? 1 : 0
                    visible: opacity > 0
                    Behavior on opacity {
                        enabled: n.p.transition !== "none"
                        NumberAnimation {
                            duration: Style.normal
                        }
                    }
                    Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData })
                }
            }
        }
    }

    // Leaves ----------------------------------------------------------------------

    Component {
        id: labelC
        // A Text that elides reports no natural width once it is given none, so the
        // natural size comes from an unconstrained copy.
        Item {
            implicitWidth: natural.implicitWidth
            implicitHeight: natural.implicitHeight
            Label {
                id: natural
                visible: false
                text: shown.text
                textFormat: shown.textFormat
                font: shown.font
                elide: Text.ElideNone
                maximumLineCount: 1000
            }
            Label {
                id: shown
                anchors.fill: parent
                text: n.p.markup ? Markup.toStyled(n.p.text || "", c => {
                    const col = Style.token(c);
                    return col === "transparent" ? "" : col.toString();
                }) : (n.p.text || "")
                textFormat: n.p.markup ? Text.StyledText : Text.PlainText
                // StyledText has no font face: a label all in <tt> takes the monospace one.
                font.family: n.p.markup && /^\s*<tt>[\s\S]*<\/tt>\s*$/.test(n.p.text || "") ? Style.mono : Style.font
                color: n.fg
                size: n.p.font_size > 0 ? n.p.font_size : Style.body
                elide: Text.ElideRight
                // A plugin's line breaks stand, as in GTK; long lines still elide.
                maximumLineCount: 1000
            }
        }
    }

    Component {
        id: iconC
        Icon {
            name: n.p.name || ""
            size: n.p.size > 0 ? n.p.size : 16
            recolor: name.endsWith("-symbolic") || !!n.p.color
            color: n.fg
        }
    }

    Component {
        id: imageC
        ClippingRectangle {
            radius: n.p.radius || 0
            color: "transparent"
            implicitWidth: img.implicitWidth
            implicitHeight: img.implicitHeight
            Image {
                id: img
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                source: n.p.data ? "data:image/png;base64," + n.p.data : (n.p.path ? "file://" + n.p.path : "")
                sourceSize: Qt.size(Math.max(1, (n.p.width || 64) * 2), Math.max(1, (n.p.height || 64) * 2))
            }
        }
    }

    Component {
        id: badgeC
        Badge {
            text: n.p.text || ""
            tone: n.has("warning") ? "warning" : n.has("danger") || n.has("urgent") ? "danger" : n.has("success") ? "success" : "accent"
        }
    }

    Component {
        id: progressC
        ProgressBar {
            value: n.p.value || 0
            implicitWidth: 60
            fill: n.p.color ? Style.token(n.p.color) : Style.accent
        }
    }

    Component {
        id: spinnerC
        Spinner {
            size: n.p.height > 0 ? Math.min(n.p.height, 24) : 16
            running: n.p.spinning !== false
        }
    }

    Component {
        id: graphC
        Canvas {
            id: g
            implicitWidth: 60
            implicitHeight: 20
            readonly property var values: n.p.values || []
            readonly property real ceiling: n.p.max > 0 ? n.p.max : Math.max(1, ...values)
            readonly property color tint: n.p.color ? Style.token(n.p.color) : Style.accent
            onValuesChanged: requestPaint()
            onTintChanged: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const v = values;
                if (v.length === 0)
                    return;
                const w = width, h = height;
                if (n.p.style === "bars") {
                    const bw = w / v.length;
                    ctx.fillStyle = tint;
                    for (let i = 0; i < v.length; i++) {
                        const bh = Math.max(1, h * Math.min(1, v[i] / ceiling));
                        ctx.fillRect(i * bw + 0.5, h - bh, Math.max(1, bw - 1), bh);
                    }
                } else {
                    ctx.strokeStyle = tint;
                    ctx.lineWidth = 1.5;
                    ctx.lineJoin = "round";
                    ctx.beginPath();
                    for (let i = 0; i < v.length; i++) {
                        const x = v.length === 1 ? w : i * w / (v.length - 1);
                        const y = h - 1 - (h - 2) * Math.min(1, v[i] / ceiling);
                        if (i === 0)
                            ctx.moveTo(x, y);
                        else
                            ctx.lineTo(x, y);
                    }
                    ctx.stroke();
                }
            }
        }
    }

    Component {
        id: vectorC
        VectorView {
            svg: n.p.svg || ""
            commands: n.p.commands || []
            viewBox: n.p.view_box || null
        }
    }

    Component {
        id: frameC
        Item {
            implicitWidth: n.p.width || 0
            implicitHeight: n.p.height || 0
        }
    }

    Component {
        id: rowC
        ListRow {
            icon: n.p.icon || ""
            title: n.p.title || ""
            subtitle: n.p.subtitle || ""
            implicitWidth: 220
            onClicked: n.ctx.emit("action", n.p.action || n.p.id || "", null)
        }
    }

    Component {
        id: toggleC
        RowLayout {
            spacing: 10
            Label {
                visible: !!n.p.label
                text: n.p.label || ""
                Layout.fillWidth: true
                color: n.fg
            }
            Toggle {
                checked: !!n.p.active
                onToggled: on => {
                    n.applySet({ active: on });
                    n.ctx.emit("toggle", n.p.id || "", on);
                }
            }
        }
    }

    Component {
        id: sliderC
        Slider {
            from: n.p.min !== undefined ? n.p.min : 0
            to: n.p.max !== undefined ? n.p.max : 100
            step: n.p.step !== undefined ? n.p.step : 1
            value: n.p.value || 0
            icon: n.p.icon || ""
            implicitWidth: 160
            onMoved: v => n.ctx.emit("change", n.p.id || "", v)
            onReleased: v => n.applySet({ value: v })
        }
    }

    Component {
        id: buttonC
        Loader {
            sourceComponent: n.p.label ? textButton : iconButton
            Component {
                id: textButton
                Button {
                    text: n.p.label || ""
                    icon: n.p.icon || ""
                    primary: n.has("accent") || n.has("suggested")
                    danger: n.has("danger")
                    confirm: false
                    onClicked: n.ctx.emit("action", n.p.action || n.p.id || "", null)
                }
            }
            Component {
                id: iconButton
                IconButton {
                    icon: n.p.icon || ""
                    toggled: n.has("active")
                    onClicked: n.ctx.emit("action", n.p.action || n.p.id || "", null)
                }
            }
        }
    }

    Component {
        id: entryC
        SearchField {
            icon: ""
            text: n.p.text || ""
            placeholder: n.p.placeholder || ""
            implicitWidth: 180
            onAccepted: n.ctx.emit("submit", n.p.id || "", text)
        }
    }

    Component {
        id: separatorC
        Separator {
            implicitWidth: 1
            implicitHeight: 1
        }
    }

    Component {
        id: scrollC
        Flickable {
            id: fl
            clip: true
            contentWidth: width
            contentHeight: inner.implicitHeight
            implicitWidth: inner.implicitWidth
            implicitHeight: n.p.max_height > 0 ? Math.min(n.p.max_height, inner.implicitHeight) : inner.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Loader {
                id: inner
                width: fl.width
                height: item ? item.implicitHeight : 0
                Component.onCompleted: if (n.kids.length)
                    setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: n.kids[0] })
            }
        }
    }
}
