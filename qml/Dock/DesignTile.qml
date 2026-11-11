import QtQuick
import Nyx.Ui

// A Design 2.0 icon drawn from the theme, as nyx-design's make_icons.py renders the app
// icons (its TEMPLATE, tile_for and glyph_colors): a satin tile, a soft glow, a hairline,
// the glyph's back at 35 % and its front in a light-to-mid gradient with the detail cut
// out. For the dock's own buttons, which have no app icon to recolour.
Item {
    id: t
    // {back, front, detail}: paths in the 128 px icon grid.
    property var glyph: ({ back: "", front: "", detail: "" })
    // The colour the glyph is drawn in (an accent or a swatch).
    property color tint: Style.accent
    property real size: 48
    implicitWidth: size
    implicitHeight: size

    function hex(c) {
        c = Style.toColor(c);
        const h = v => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, "0");
        return "#" + h(c.r) + h(c.g) + h(c.b);
    }
    readonly property var tile: Style.light
        ? { top: Style.mix(Style.bg, "#ffffff", 0.7), bottom: Style.mix(Style.bg, Style.text, 0.07), cut: Style.mix(Style.bg, "#ffffff", 0.5) }
        : { top: Style.mix(Style.bg, Style.text, 0.13), bottom: Style.mix(Style.bg, Style.text, 0.04), cut: Style.mix(Style.bg, Style.text, 0.05) }
    readonly property string light: hex(Style.mix(tint, "#ffffff", 0.28))
    readonly property string mid: hex(Style.mix(tint, "#000000", 0.08))
    readonly property string svg: '<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><defs>'
        + '<linearGradient id="tile" x1="0" y1="0" x2="1" y2="1"><stop stop-color="' + hex(tile.top) + '"/><stop offset="1" stop-color="' + hex(tile.bottom) + '"/></linearGradient>'
        + '<radialGradient id="glow" cx=".2" cy=".1" r=".9"><stop stop-color="' + light + '" stop-opacity=".22"/><stop offset="1" stop-color="' + light + '" stop-opacity="0"/></radialGradient>'
        + '<linearGradient id="ink" x1="0" y1="0" x2="1" y2="1"><stop stop-color="' + light + '"/><stop offset="1" stop-color="' + mid + '"/></linearGradient></defs>'
        + '<rect x="8" y="8" width="112" height="112" rx="28" fill="url(#tile)"/>'
        + '<rect x="8" y="8" width="112" height="112" rx="28" fill="url(#glow)"/>'
        + '<rect x="8.5" y="8.5" width="111" height="111" rx="27.5" fill="none" stroke="' + light + '" stroke-opacity=".3"/>'
        + '<path d="' + (glyph.back || "") + '" fill="' + light + '" fill-opacity=".35"/>'
        + '<path d="' + (glyph.front || "") + '" fill="url(#ink)"/>'
        + '<path d="' + (glyph.detail || "") + '" fill="none" stroke="' + hex(tile.cut) + '" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/></svg>'

    Image {
        anchors.fill: parent
        source: "data:image/svg+xml;utf8," + encodeURIComponent(t.svg)
        sourceSize: Qt.size(t.size * 2, t.size * 2)
        smooth: true
    }
}
