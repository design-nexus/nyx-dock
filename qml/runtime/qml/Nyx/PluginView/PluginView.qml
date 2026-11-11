import QtQuick
import "style.js" as Css

// A plugin's view (nyx-plugin-protocol's Node tree, the JSON a plugin sends) drawn
// with Nyx.Ui, for QML hosts: Nyx Bar, the Control Center's plugin tiles and the
// desktop widgets. The QML counterpart of nyx-design's nyx-plugin-view.
//
//   PluginView {
//       view: tile.view                     // the item's tree, paths already resolved
//       onViewEvent: (kind, control, value) => backend.request("PluginEvent", {…})
//   }
//   … and on a `patch` message: view.patch(nodes)   // [{id, set: {…}}]
//
// The backend resolves `/plugin/…` and `/storage/…` paths before the tree gets here and
// leaves out files a third-party plugin may not show. Plugin CSS applies when the host
// passes it as `rules` (compiled, see style.js); Pango markup is shown through
// StyledText (markup.js); `frame` nodes show nothing.
Item {
    id: root

    property var view: null
    // The largest height inline nodes may take (a bar's height); 0 for none.
    property int maxHeight: 0
    // Style rules (style.js): a host's classes, then the plugin's style.css. With none,
    // nodes take only the few classes this view knows (dim, accent, …).
    property var rules: []
    readonly property bool hoverRules: Css.anyHover(rules)
    // The host's element around the view (`&` in a plugin's CSS): its classes, whether
    // the pointer is over it, and the text settings it passes down.
    property var rootClasses: []
    property bool rootHovered: false
    property var rootText: ({})
    // Figures of one width in every label (the bar's clock and readouts).
    property bool tabular: false
    // Nodes' tooltips open below them (a bar along the top of the screen).
    property bool tipBelow: false
    // Nodes with a tooltip under the pointer now.
    property int tipHovers: 0
    // How much of the spare width of labels marked `shrink` to give up, 0 to 1, and that
    // spare width in all.
    property real squeeze: 0
    property real spare: 0
    property var _spares: []
    function setSpare(obj, v) {
        const list = _spares.filter(e => e.o !== obj);
        if (v > 0)
            list.push({ o: obj, v: v });
        _spares = list;
        spare = list.reduce((a, e) => a + e.v, 0);
    }

    // A view of another shape replaced the old one: its nodes are built anew. A view of
    // the same shape (the item sent again with new values) updates them in place, so
    // icons don't blink, sliders keep their drag and scrolled lists their place.
    property int generation: 0
    property string _shape: ""
    property var _ids: ({})
    function shape(v) {
        if (!v)
            return "";
        const kids = v.children || (v.child ? [v.child] : []);
        return (v.node || "box") + "#" + (v.id || "") + "(" + kids.map(shape).join(",") + ")";
    }

    signal viewEvent(string kind, string control, var value)

    function register(id, obj) {
        if (id)
            _ids[id] = obj;
    }
    function unregister(id, obj) {
        if (id && _ids[id] === obj)
            delete _ids[id];
    }
    // A plugin's `patch`: properties set on nodes by id.
    function patch(nodes) {
        for (const p of nodes || []) {
            const obj = _ids[p.id];
            if (obj)
                obj.applySet(p.set || {});
        }
    }
    function emit(kind, control, value) {
        viewEvent(kind, control || "", value === undefined ? null : value);
    }

    onViewChanged: {
        const sh = shape(view);
        if (sh === _shape)
            return;
        _shape = sh;
        _ids = {};
        generation += 1;
    }

    implicitWidth: loader.item ? loader.item.implicitWidth : 0
    implicitHeight: loader.item ? loader.item.implicitHeight : 0

    Loader {
        id: loader
        anchors.fill: parent
        active: root.view !== null && root.view !== undefined
        sourceComponent: Component {
            PluginNode {
                ctx: root
                node: root.view
            }
        }
        // A new tree builds new nodes, so patches find the new ones.
        property int built: root.generation
        onBuiltChanged: {
            active = false;
            active = Qt.binding(() => root.view !== null && root.view !== undefined);
        }
    }
}
