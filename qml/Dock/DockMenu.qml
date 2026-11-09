import QtQuick
import QtQuick.Dialogs
import Quickshell
import Nyx.Ui

// The dock's own settings: icon size, position, hiding, previews, the Apps button,
// names in the Apps panel, the Trash; add a folder; Nyx Settings' Dock page.
DockPopup {
    id: p

    anchorTo: anchorItem
    property Item anchorItem
    readonly property var s: w.settings
    pad: 14

    function set(key, value) {
        w.call("Set", { key: key, value: value });
    }

    Column {
        width: 300
        spacing: 12
        Label {
            text: "Dock"
            font.weight: Font.DemiBold
        }
        Column {
            width: parent.width
            Label {
                text: "Size"
                role: "secondary"
                dim: true
            }
            Slider {
                width: parent.width
                from: 24
                to: 96
                step: 4
                value: p.s.icon_size || 48
                onReleased: v => p.set("icon_size", Math.round(v))
            }
        }
        Row {
            spacing: 8
            Label {
                anchors.verticalCenter: parent.verticalCenter
                width: 70
                text: "Position"
                role: "secondary"
                dim: true
            }
            Segmented {
                width: 220
                options: [{ id: "bottom", icon: "go-down-symbolic" }, { id: "left", icon: "go-previous-symbolic" }, { id: "right", icon: "go-next-symbolic" }, { id: "top", icon: "go-up-symbolic" }]
                current: p.s.position || "bottom"
                onChosen: id => p.set("position", id)
            }
        }
        Row {
            spacing: 8
            Label {
                anchors.verticalCenter: parent.verticalCenter
                width: 70
                text: "Hide"
                role: "secondary"
                dim: true
            }
            Segmented {
                width: 220
                options: [{ id: "smart", label: "Smart" }, { id: "always", label: "Always" }, { id: "never", label: "Never" }]
                current: p.s.hide || "smart"
                onChosen: id => p.set("hide", id)
            }
        }
        Repeater {
            model: [["previews", "Window previews"], ["apps", "Apps button"], ["app_names", "App names"], ["trash", "Trash"]]
            delegate: Item {
                required property var modelData
                width: parent.width
                height: 30
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData[1]
                }
                Toggle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    checked: p.w.global[modelData[0]] !== false
                    onToggled: on => p.set(modelData[0], on)
                }
            }
        }
        Row {
            spacing: 6
            Button {
                text: "Add a folder"
                icon: "folder-new-symbolic"
                onClicked: folderDialog.open()
            }
            Button {
                visible: !!p.w.global.settings_app
                text: "Dock settings"
                icon: "emblem-system-symbolic"
                onClicked: {
                    p.close();
                    p.w.call("DockSettings", {});
                }
            }
        }
    }
    FolderDialog {
        id: folderDialog
        title: "Add a Folder to the Dock"
        acceptLabel: "Add"
        currentFolder: "file://" + Quickshell.env("HOME")
        onAccepted: p.w.call("AddFolders", { uris: [selectedFolder.toString()] })
    }
}
