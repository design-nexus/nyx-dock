import QtQuick
import Nyx.Ui

// A plugin's drawing: an SVG document, or drawing commands in `viewBox` units scaled
// to the node's size (nyx-bar's docs/plugins.md, Vector drawings).
Item {
    id: v

    property string svg: ""
    property var commands: []
    property var viewBox: null

    implicitWidth: viewBox ? viewBox[2] : 24
    implicitHeight: viewBox ? viewBox[3] : 24

    Image {
        anchors.fill: parent
        visible: v.svg !== ""
        source: v.svg !== "" ? "data:image/svg+xml;utf8," + encodeURIComponent(v.svg) : ""
        sourceSize: Qt.size(Math.max(1, v.width * 2), Math.max(1, v.height * 2))
        fillMode: Image.PreserveAspectFit
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        visible: v.svg === ""
        antialiasing: true

        function css(c) {
            const col = Style.toColor(Style.token(c));
            return "rgba(" + Math.round(col.r * 255) + "," + Math.round(col.g * 255) + "," + Math.round(col.b * 255) + "," + col.a + ")";
        }
        function brush(ctx, p) {
            if (p === undefined || p === null)
                return null;
            if (typeof p === "string")
                return css(p);
            if (p.linear) {
                const g = ctx.createLinearGradient(p.linear.x1, p.linear.y1, p.linear.x2, p.linear.y2);
                for (const s of p.linear.stops || [])
                    g.addColorStop(s[0], css(s[1]));
                return g;
            }
            if (p.radial) {
                const g = ctx.createRadialGradient(p.radial.cx, p.radial.cy, 0, p.radial.cx, p.radial.cy, p.radial.r);
                for (const s of p.radial.stops || [])
                    g.addColorStop(s[0], css(s[1]));
                return g;
            }
            return null;
        }
        function finish(ctx, c) {
            const f = brush(ctx, c.fill), s = brush(ctx, c.stroke);
            if (f) {
                ctx.fillStyle = f;
                ctx.fill();
            }
            if (s) {
                ctx.strokeStyle = s;
                ctx.lineWidth = c.width || 1;
                ctx.lineCap = c.round ? "round" : "butt";
                ctx.stroke();
            }
        }

        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const vb = v.viewBox || [0, 0, width, height];
            ctx.save();
            ctx.scale(width / Math.max(1e-6, vb[2]), height / Math.max(1e-6, vb[3]));
            ctx.translate(-vb[0], -vb[1]);
            let n = 0;
            for (const c of v.commands) {
                if (++n > 2000)
                    break;
                switch (c.op) {
                case "path":
                    ctx.beginPath();
                    ctx.path = c.d || "";
                    finish(ctx, c);
                    break;
                case "line":
                    ctx.beginPath();
                    ctx.moveTo(c.x1, c.y1);
                    ctx.lineTo(c.x2, c.y2);
                    finish(ctx, { stroke: c.stroke || "text", width: c.width, round: c.round });
                    break;
                case "arc":
                    ctx.beginPath();
                    ctx.arc(c.cx, c.cy, c.r, (c.start || 0) * Math.PI / 180, (c.end || 0) * Math.PI / 180, false);
                    finish(ctx, c);
                    break;
                case "circle":
                    ctx.beginPath();
                    ctx.arc(c.cx, c.cy, c.r, 0, Math.PI * 2, false);
                    finish(ctx, c);
                    break;
                case "rect":
                    ctx.beginPath();
                    if (c.radius > 0)
                        ctx.roundedRect(c.x, c.y, c.w, c.h, c.radius, c.radius);
                    else
                        ctx.rect(c.x, c.y, c.w, c.h);
                    finish(ctx, c);
                    break;
                case "text":
                    ctx.font = (c.bold ? "bold " : "") + (c.size || 12) + "px \"" + Style.font + "\"";
                    ctx.textAlign = ({ center: "center", end: "end" })[c.align] || "start";
                    ctx.textBaseline = "alphabetic";
                    ctx.fillStyle = brush(ctx, c.fill) || css("text");
                    ctx.fillText(c.text || "", c.x, c.y);
                    break;
                case "save":
                    ctx.save();
                    break;
                case "restore":
                    ctx.restore();
                    break;
                case "translate":
                    ctx.translate(c.x || 0, c.y || 0);
                    break;
                case "rotate":
                    ctx.rotate((c.deg || 0) * Math.PI / 180);
                    break;
                case "scale":
                    ctx.scale(c.x || 1, c.y || 1);
                    break;
                }
            }
            ctx.restore();
        }
    }

    onCommandsChanged: canvas.requestPaint()
    onViewBoxChanged: canvas.requestPaint()
    Connections {
        target: Style
        function onAccentChanged() {
            canvas.requestPaint();
        }
        function onTextChanged() {
            canvas.requestPaint();
        }
    }
}
