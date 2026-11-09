import QtQuick
import Quickshell
import Nyx.Shell

// Nyx Dock in the shared Quickshell runtime: one dock per screen its backend
// (nyx-dock-backend) shows it on. The backend decides what's on it and when it hides;
// this draws it and tells the backend where the bar is and what keeps it up.
Scope {
    id: root

    readonly property string pinnedScreen: Quickshell.env("NYX_SHELL_SCREEN") || ""
    readonly property var dock: backend.state.dock || ({ monitors: [], global: ({}), folders: [], trash: 0 })

    BackendClient {
        id: backend
        app: "nyx-dock"
        topics: ["dock", "controls"]
    }

    WindowControls {
        client: backend
    }

    Variants {
        model: root.dock.monitors.filter(m => (root.pinnedScreen === "" || m.name === root.pinnedScreen) && Quickshell.screens.some(s => s.name === m.name)).map(m => m.name)
        delegate: DockWindow {
            required property string modelData
            client: backend
            name: modelData
            state_: root.dock
        }
    }
}
