.pragma library

// Pango markup (what plugin views use) as Qt's StyledText, which has no <span>:
// `size`, `weight`, `style`, `foreground`/`color` and `alpha` on a span become <small>,
// <big>, <b>, <i> and <font color>; the plain tags that both know pass through.
// Anything StyledText can't show is dropped, keeping the text. `colorOf(name)` turns a
// colour attribute (a token or #rrggbb) into a CSS colour, or "" to leave it.

function escapeText(s) {
    return String(s).replace(/&(?!(amp|lt|gt|quot|apos|#[0-9]+|#x[0-9a-fA-F]+);)/g, "&amp;");
}

function attrs(s) {
    var out = {};
    var re = /([a-z_]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
    var m;
    while ((m = re.exec(s)) !== null)
        out[m[1]] = m[3] !== undefined ? m[3] : m[4];
    return out;
}

var PASS = { b: 1, i: 1, u: 1, s: 1, small: 1, big: 1, sub: 1, sup: 1, tt: 1 };

function toStyled(markup, colorOf) {
    var out = "";
    var stack = [];
    var re = /<(\/?)([a-zA-Z]+)([^>]*)>/g;
    var last = 0;
    var m;
    var text = String(markup);
    while ((m = re.exec(text)) !== null) {
        out += text.substring(last, m.index);
        last = re.lastIndex;
        var closing = m[1] === "/";
        var tag = m[2].toLowerCase();
        if (tag === "span") {
            if (closing) {
                var closes = stack.pop() || [];
                for (var i = closes.length - 1; i >= 0; i--)
                    out += "</" + closes[i] + ">";
                continue;
            }
            var a = attrs(m[3]);
            var opened = [];
            var size = a.size || a.font_size;
            if (size === "small" || size === "x-small" || size === "xx-small" || size === "smaller") {
                out += "<small>";
                opened.push("small");
            } else if (size === "large" || size === "x-large" || size === "xx-large" || size === "larger") {
                out += "<big>";
                opened.push("big");
            }
            var weight = a.weight || a.font_weight;
            if (weight === "bold" || weight === "heavy" || weight === "ultrabold" || weight === "semibold" || Number(weight) >= 600) {
                out += "<b>";
                opened.push("b");
            }
            if ((a.style || a.font_style) === "italic") {
                out += "<i>";
                opened.push("i");
            }
            var col = a.foreground || a.fgcolor || a.color;
            if (col) {
                var c = colorOf(col);
                if (c) {
                    out += "<font color=\"" + c + "\">";
                    opened.push("font");
                }
            }
            stack.push(opened);
        } else if (PASS[tag]) {
            out += "<" + m[1] + tag + ">";
        }
    }
    out += text.substring(last);
    while (stack.length) {
        var rest = stack.pop();
        for (var j = rest.length - 1; j >= 0; j--)
            out += "</" + rest[j] + ">";
    }
    return out.replace(/\n/g, "<br>");
}
