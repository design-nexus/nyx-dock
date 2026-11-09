import QtQuick

// A plugin's view (nyx-plugin-protocol's Node tree, the JSON a plugin sends) drawn
// with Nyx.Ui, for QML hosts: the Control Center's plugin tiles and the desktop
// widgets. The QML counterpart of nyx-design's nyx-plugin-view.
//
//   PluginView {
//       view: tile.view                     // the item's tree, paths already resolved
//       onViewEvent: (kind, control, value) => backend.request("PluginEvent", {…})
//   }
//   … and on a `patch` message: view.patch(nodes)   // [{id, set: {…}}]
//
// The backend resolves `/plugin/…` and `/storage/…` paths before the tree gets here and
// leaves out files a third-party plugin may not show. Plugin CSS (style.css) isn't
// applied; Pango markup is shown through StyledText (markup.js); `frame` nodes show
// nothing.
Item {
    id: root

    property var view: null
    // The largest height inline nodes may take (a bar's height); 0 for none.
    property int maxHeight: 0

    // A view with this kind of tree replaced the old one (the item was sent again).
    property int generation: 0
    property var _ids: ({})

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
