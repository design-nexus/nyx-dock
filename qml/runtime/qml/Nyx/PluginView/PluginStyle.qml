pragma Singleton

import QtQuick
import Quickshell
import Nyx.Ui
import "style.js" as Css

// Plugin CSS for hosts: the rules that match an element (style.js), and a rule's
// colour as a QML colour. The bar uses it for its items' own look (`&.bar-item`).
Singleton {
    function compute(rules, chain) {
        return Css.compute(rules, chain);
    }

    function anyHover(rules) {
        return Css.anyHover(rules);
    }

    // An icon name may list fallbacks, "a-symbolic|b|c": the first the icon theme has (a
    // path or a single name as it is).
    function icon(name) {
        if (!name || name.indexOf("|") < 0)
            return name || "";
        const names = name.split("|").map(s => s.trim()).filter(s => s !== "");
        for (const n of names)
            if ((n.startsWith("/") && !n.endsWith("-symbolic")) || Quickshell.iconPath(n, true) !== "")
                return n;
        return names.length ? names[names.length - 1] : "";
    }

    // `{t}` a theme colour, `{hex}`, `{mix: [a, b, t]}`, with `a` (alpha) and `shade`.
    function color(c) {
        if (!c || c.none)
            return "transparent";
        let col;
        if (c.mix)
            col = Style.mix(Style.toColor(color(c.mix[0])), Style.toColor(color(c.mix[1])), c.mix[2]);
        else
            col = Style.toColor(c.hex ? Style.token(c.hex) : Style.token(c.t));
        if (c.shade)
            col = c.shade >= 1 ? Qt.lighter(col, c.shade) : Qt.darker(col, 1 / c.shade);
        return c.a !== undefined ? Style.alpha(col, c.a) : col;
    }
}
