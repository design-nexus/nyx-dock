import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets
import Nyx.Ui
import "markup.js" as Markup
import "style.js" as Css

// One node of a plugin view and, through Loaders, its children. `node` is the node as
// sent; patches change `base`; `p` is what is drawn (base plus the hover and pressed
// property sets while they apply). Children come from the node as sent: patches
// never change them.
//
// With style rules (`ctx.rules`: a host's classes and the plugin's style.css, see
// style.js), a node takes the properties of the rules that match it and its ancestors:
// padding, margins, minimum sizes, background, corners, an inset ring, a top border,
// opacity, and the text's colour, size, weight and spacing (those four pass down to
// children, as in CSS).
Item {
    id: n

    property var ctx: null
    property var node: ({})
    // The parent node, for selectors that name ancestors.
    property var up: null
    property var base: node
    property bool hov: false
    property bool prs: false
    // Under the pointer, for :hover rules.
    property bool hovering: false
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
    readonly property var classes: p["class"] || []

    function has(c) {
        return classes.indexOf(c) >= 0;
    }

    // The node, its ancestors and the host's root, for the rules.
    function chain() {
        const out = [{ kind: kind, classes: classes, hovered: hovering }];
        for (let x = up; x; x = x.up)
            out.push({ kind: x.kind, classes: x.classes, hovered: x.hovering });
        out.push({ root: true, classes: ctx ? (ctx.rootClasses || []) : [], hovered: ctx ? !!ctx.rootHovered : false });
        return out;
    }
    readonly property bool styled: !!(ctx && ctx.rules && ctx.rules.length)
    readonly property var st: styled ? Css.compute(ctx.rules, chain()) : ({})
    function side(key, i) {
        const one = st[key + ["-top", "-right", "-bottom", "-left"][i]];
        if (one !== undefined)
            return one;
        return st[key] ? st[key][i] : 0;
    }
    readonly property var pad: [side("padding", 0), side("padding", 1), side("padding", 2), side("padding", 3)]
    readonly property var m: {
        const own = (p.margin && p.margin.length === 4) ? p.margin : [0, 0, 0, 0];
        return [own[0] + side("margin", 0), own[1] + side("margin", 1), own[2] + side("margin", 2), own[3] + side("margin", 3)];
    }
    function cssColor(c) {
        return PluginStyle.color(c);
    }
    // What passes down: colour and text settings, the parent's then this node's.
    readonly property var inherited: up ? up.passed : (ctx && ctx.rootText ? ctx.rootText : ({}))
    readonly property var passed: {
        const out = Object.assign({}, inherited);
        for (const k of ["color", "font-size", "font-weight", "letter-spacing"])
            if (st[k] !== undefined)
                out[k] = st[k];
        return out;
    }
    readonly property real textSize: p.font_size > 0 ? p.font_size : (passed["font-size"] || Style.body)
    readonly property int textWeight: passed["font-weight"] || 400
    readonly property real textSpacing: passed["letter-spacing"] || 0
    readonly property color fg: {
        if (p.color)
            return Style.token(p.color);
        if (passed.color)
            return cssColor(passed.color);
        if (styled)
            return Style.text;
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
    opacity: st.opacity !== undefined ? st.opacity : 1
    // As in GTK: a node's width and height are its smallest size, padding included (not
    // its margins), and it grows to fit what it holds; CSS min sizes leave the padding out.
    // A drawing, picture or graph is exactly the size it asks for.
    readonly property bool exact: ["graph", "vector", "image", "frame", "spinner", "progress"].indexOf(kind) >= 0
    readonly property real innerW: exact && p.width > 0 ? p.width - pad[1] - pad[3] : Math.max(p.width > 0 ? p.width - pad[1] - pad[3] : 0, st["min-width"] || 0, content.item ? content.item.implicitWidth : 0)
    readonly property real innerH: exact && p.height > 0 ? p.height - pad[0] - pad[2] : Math.max(p.height > 0 ? p.height - pad[0] - pad[2] : 0, st["min-height"] || 0, content.item ? content.item.implicitHeight : 0)
    implicitWidth: innerW + pad[1] + pad[3] + m[1] + m[3]
    implicitHeight: Math.min(ctx && ctx.maxHeight > 0 ? ctx.maxHeight : 1e9, innerH + pad[0] + pad[2] + m[0] + m[2])

    Rectangle {
        x: n.m[3]
        y: n.m[0]
        width: n.width - n.m[1] - n.m[3]
        height: n.height - n.m[0] - n.m[2]
        readonly property var ring: n.st.ring && !n.st.ring.none ? n.st.ring : (n.st.border || null)
        visible: !!n.p.background || !!n.st["background-color"] || !!ring
        color: n.p.background ? Style.token(n.p.background) : n.cssColor(n.st["background-color"])
        radius: n.st["border-radius"] !== undefined ? n.st["border-radius"] : (n.p.background ? Style.radiusSmall : 0)
        border.width: ring ? ring.w : 0
        border.color: ring ? n.cssColor(ring.c) : "transparent"
        Rectangle {
            visible: !!n.st["border-top"]
            width: parent.width
            height: n.st["border-top"] ? n.st["border-top"].w : 0
            color: n.st["border-top"] ? n.cssColor(n.st["border-top"].c) : "transparent"
        }
        Rectangle {
            visible: !!n.st["border-left"]
            width: n.st["border-left"] ? n.st["border-left"].w : 0
            height: parent.height
            color: n.st["border-left"] ? n.cssColor(n.st["border-left"].c) : "transparent"
        }
    }

    Loader {
        id: content
        x: n.m[3] + n.pad[3]
        y: n.m[0] + n.pad[0]
        width: n.width - n.m[1] - n.m[3] - n.pad[1] - n.pad[3]
        height: n.height - n.m[0] - n.m[2] - n.pad[0] - n.pad[2]
        sourceComponent: (n.kind === "icon" || n.kind === "image") && n.has("mono") ? monoC : ({
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
    HoverHandler {
        enabled: n.styled && !!n.ctx.hoverRules
        onHoveredChanged: n.hovering = hovered
    }
    // Any node with an action is clickable, as a button is.
    TapHandler {
        enabled: !!n.p.action && n.kind !== "button" && n.kind !== "row"
        cursorShape: Qt.PointingHandCursor
        onTapped: n.ctx.emit("action", n.p.action, null)
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
        // A hidden node takes no room or spacing in its box.
        visible: modelData.visible !== false
        Layout.fillWidth: horizontal ? !!modelData.expand : (align === "fill" && !(modelData.width > 0))
        Layout.fillHeight: horizontal ? (align === "fill" && !(modelData.height > 0)) : !!modelData.expand
        Layout.alignment: horizontal ? ({ start: Qt.AlignTop, end: Qt.AlignBottom })[align] || Qt.AlignVCenter : ({ start: Qt.AlignLeft, end: Qt.AlignRight })[align] || Qt.AlignHCenter
        Layout.preferredWidth: item ? item.implicitWidth : 0
        Layout.preferredHeight: item ? item.implicitHeight : 0
        Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData, up: n })
    }

    Component {
        id: boxC
        Item {
            implicitWidth: grid.implicitWidth
            implicitHeight: grid.implicitHeight
            // A row wider than its children keeps them at its start, as GTK packs them,
            // unless one expands to take the room.
            readonly property bool packs: grid.horizontal && !n.kids.some(k => k.expand)
            GridLayout {
                id: grid
                readonly property bool horizontal: n.p.orientation !== "vertical"
                width: parent.packs ? Math.min(parent.width, implicitWidth) : parent.width
                height: parent.height
                flow: horizontal ? GridLayout.LeftToRight : GridLayout.TopToBottom
                rows: horizontal ? 1 : -1
                columns: horizontal ? -1 : 1
                rowSpacing: n.p.spacing !== undefined ? n.p.spacing : 4
                columnSpacing: rowSpacing
                Repeater {
                    model: n.kids
                    delegate: Child {
                        horizontal: grid.horizontal
                        align: n.p.align || "center"
                    }
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
                    Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData, up: n })
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
                    Component.onCompleted: setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: modelData, up: n })
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
            // Out of the bar a long label wraps onto more lines, as in GTK.
            readonly property bool wraps: !(n.ctx && n.ctx.maxHeight > 0)
            implicitWidth: natural.implicitWidth
            implicitHeight: wraps ? shown.implicitHeight : natural.implicitHeight
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
                width: parent.width
                height: parent.wraps ? Math.max(implicitHeight, parent.height) : parent.height
                wrapMode: parent.wraps ? Text.Wrap : Text.NoWrap
                text: n.p.markup ? Markup.toStyled(n.p.text || "", c => {
                    const col = Style.token(c);
                    return col === "transparent" ? "" : col.toString();
                }) : (n.p.text || "")
                textFormat: n.p.markup ? Text.StyledText : Text.PlainText
                // StyledText has no font face: a label all in <tt> takes the monospace one.
                font.family: n.p.markup && /^\s*<tt>[\s\S]*<\/tt>\s*$/.test(n.p.text || "") ? Style.mono : Style.font
                font.weight: n.textWeight
                font.letterSpacing: n.textSpacing
                font.features: n.ctx && n.ctx.tabular ? ({ "tnum": 1 }) : ({})
                color: n.fg
                size: n.textSize
                horizontalAlignment: n.has("center") ? Text.AlignHCenter : Text.AlignLeft
                elide: Text.ElideRight
                // A plugin's line breaks stand, as in GTK; long lines still elide.
                maximumLineCount: 1000
            }
        }
    }

    Component {
        id: iconC
        Icon {
            name: PluginStyle.icon(n.p.name)
            size: n.p.size > 0 ? n.p.size : (n.st["icon-size"] || 16)
            recolor: name.endsWith("-symbolic") || !!n.p.color
            color: n.fg
            rotation: n.st.rotate || 0
        }
    }

    // An app's own picture that is all one neutral colour (a white tray icon; the host
    // marks it `mono`), in the text colour.
    Component {
        id: monoC
        MonoTint {
            size: n.kind === "icon" ? (n.p.size > 0 ? n.p.size : (n.st["icon-size"] || 16)) : (n.p.width || 16)
            color: n.fg
            source: {
                if (n.kind === "image")
                    return n.p.data ? "data:image/png;base64," + n.p.data : (n.p.path ? "file://" + n.p.path : "");
                const name = PluginStyle.icon(n.p.name);
                return name.startsWith("/") ? "file://" + name : Quickshell.iconPath(name, true);
            }
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
                    // The line, and under it the same colour at 18 %, as the GTK view draws it.
                    ctx.lineWidth = 1.4;
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
                    ctx.strokeStyle = tint;
                    ctx.stroke();
                    ctx.lineTo(w, h);
                    ctx.lineTo(0, h);
                    ctx.closePath();
                    ctx.fillStyle = Qt.rgba(tint.r, tint.g, tint.b, 0.18);
                    ctx.fill();
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
            // With style rules the CSS draws the button (its padding, background, the
            // hover); this is only its icon and label, and the click.
            sourceComponent: n.styled ? cssButton : n.p.label ? textButton : iconButton
            Component {
                id: cssButton
                Item {
                    implicitWidth: row.implicitWidth
                    implicitHeight: Math.max(row.implicitHeight, 16)
                    Row {
                        id: row
                        anchors.centerIn: parent
                        spacing: 6
                        Icon {
                            visible: !!n.p.icon
                            anchors.verticalCenter: parent.verticalCenter
                            name: PluginStyle.icon(n.p.icon)
                            size: n.st["icon-size"] || 16
                            recolor: true
                            color: n.fg
                        }
                        Label {
                            visible: !!n.p.label
                            anchors.verticalCenter: parent.verticalCenter
                            text: n.p.label || ""
                            color: n.fg
                            size: n.textSize
                            font.weight: n.textWeight
                            font.underline: n.has("link") && n.hovering
                        }
                    }
                    TapHandler {
                        cursorShape: Qt.PointingHandCursor
                        onTapped: n.ctx.emit("action", n.p.action || n.p.id || "", null)
                    }
                }
            }
            Component {
                id: textButton
                Button {
                    text: n.p.label || ""
                    icon: n.p.icon || ""
                    primary: n.has("accent") || n.has("suggested")
                    toggled: n.has("active")
                    danger: n.has("danger")
                    confirm: false
                    onClicked: n.ctx.emit("action", n.p.action || n.p.id || "", null)
                }
            }
            Component {
                id: iconButton
                IconButton {
                    icon: PluginStyle.icon(n.p.icon)
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
            // Styled, the node's own background is the line.
            visible: !n.st["background-color"]
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
                    setSource(Qt.resolvedUrl("PluginNode.qml"), { ctx: n.ctx, node: n.kids[0], up: n })
            }
        }
    }
}
