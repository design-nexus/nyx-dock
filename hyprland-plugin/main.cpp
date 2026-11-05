// Nyx Dock's Hyprland plugin: tells the dock how far a workspace slide has gone, so it
// can slide out (or in) with a trackpad swipe instead of waiting for it to end.
//
// It only reads Hyprland's state. While a workspace on a monitor is moving (a swipe,
// or a slide animation), each frame it posts
//     nyxswipe>>MONITOR,FROM_ID,TO_ID,PROGRESS
// where PROGRESS (0 to 1) is how far FROM, the workspace the slide started on, has
// moved out of view, and once nothing moves any more
//     nyxswipeend>>MONITOR,ACTIVE_ID

#define WLR_USE_UNSTABLE

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/desktop/Workspace.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/managers/EventManager.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/managers/input/UnifiedWorkspaceSwipeGesture.hpp>
#include <hyprland/src/state/MonitorState.hpp>
#include <hyprland/src/state/WorkspaceState.hpp>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <unordered_map>
#include <vector>

namespace {

    struct Slide {
        bool        active = false;
        WORKSPACEID from   = WORKSPACE_INVALID;
        WORKSPACEID to     = WORKSPACE_INVALID;
        double      last   = -1;
    };

    // By monitor name.
    std::unordered_map<std::string, Slide> slides;
    CHyprSignalListener                    renderPre;
    CHyprSignalListener                    swipeEnd;
    // Dropped on unload, so a pending look never runs code that's gone.
    UP<SEventLoopDoLaterLock> afterSwipe;

    void post(const char* event, const std::string& data) {
        if (g_pEventManager)
            g_pEventManager->postEvent(SHyprIPCEvent{event, data});
    }

    std::vector<PHLWORKSPACE> workspacesOn(const PHLMONITOR& mon) {
        std::vector<PHLWORKSPACE> out;
        for (const auto& ref : State::workspaceState()->workspaceRefs()) {
            auto ws = ref.lock();
            if (ws && !ws->inert() && ws->m_renderOffset && !ws->m_isSpecialWorkspace && ws->m_monitor.lock() == mon)
                out.push_back(ws);
        }
        return out;
    }

    bool drawn(const PHLWORKSPACE& ws, const PHLMONITOR& mon) {
        return ws->m_visible || ws->m_forceRendering || ws == mon->m_activeWorkspace;
    }

    void check(const PHLMONITOR& mon) {
        if (!mon || !mon->m_activeWorkspace)
            return;
        const auto on     = workspacesOn(mon);
        bool       moving = false;
        for (const auto& ws : on) {
            const auto off = ws->m_renderOffset->value();
            if (ws->m_renderOffset->isBeingAnimated() || (drawn(ws, mon) && (std::abs(off.x) > 0.5 || std::abs(off.y) > 0.5))) {
                moving = true;
                break;
            }
        }
        const bool gesture = g_pUnifiedWorkspaceSwipe && g_pUnifiedWorkspaceSwipe->isGestureInProgress();
        auto&      slide   = slides[mon->m_name];
        if (!slide.active) {
            if (!moving)
                return;
            slide = Slide{.active = true, .from = mon->m_activeWorkspace->m_id};
        }
        if (!moving && !gesture) {
            post("nyxswipeend", mon->m_name + "," + std::to_string(mon->m_activeWorkspace->m_id));
            slide = Slide{};
            return;
        }

        const auto from = std::ranges::find_if(on, [&](const auto& ws) { return ws->m_id == slide.from; });
        if (from == on.end())
            return;
        const auto   fromOff  = (*from)->m_renderOffset->value();
        const bool   vertical = std::abs(fromOff.y) > std::abs(fromOff.x);
        const double extent   = vertical ? mon->m_size.y : mon->m_size.x;
        const double along    = vertical ? fromOff.y : fromOff.x;
        if (extent <= 0)
            return;
        const double progress = std::clamp(std::abs(along) / extent, 0.0, 1.0);

        // The workspace coming in: drawn, on the other side of FROM (or already in
        // place), the nearest one.
        double best = INFINITY;
        for (const auto& ws : on) {
            if (ws->m_id == slide.from || !drawn(ws, mon))
                continue;
            const auto   off = ws->m_renderOffset->value();
            const double a   = vertical ? off.y : off.x;
            if (along != 0 && a * along > 0)
                continue;
            if (std::abs(a) < best) {
                best     = std::abs(a);
                slide.to = ws->m_id;
            }
        }
        if (slide.to == WORKSPACE_INVALID || std::abs(progress - slide.last) < 0.005)
            return;
        slide.last = progress;
        char p[16];
        std::snprintf(p, sizeof(p), "%.3f", progress);
        post("nyxswipe", mon->m_name + "," + std::to_string(slide.from) + "," + std::to_string(slide.to) + "," + p);
    }

}

APICALL EXPORT std::string PLUGIN_API_VERSION() {
    return HYPRLAND_API_VERSION;
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    if (std::string{__hyprland_api_get_hash()} != __hyprland_api_get_client_hash())
        throw std::runtime_error("[nyx-dock-swipe] built for another Hyprland version; rebuild it with Nyx Dock's install.sh");

    renderPre = Event::bus()->m_events.render.pre.listen([](const PHLMONITOR& mon) { check(mon); });
    // A swipe that ends without a slide back may draw no further frame: look once
    // Hyprland has handled the end.
    swipeEnd = Event::bus()->m_events.gesture.swipe.end.listen([] {
        if (!g_pEventLoopManager)
            return;
        afterSwipe = g_pEventLoopManager->doLaterLock([] {
            for (const auto& mon : State::monitorState()->monitors()) {
                if (mon && slides[mon->m_name].active)
                    check(mon);
            }
        });
    });

    return {"nyx-dock-swipe", "Tells Nyx Dock how far a workspace swipe has gone", "Design Nexus", "1.0"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    renderPre.reset();
    swipeEnd.reset();
    afterSwipe.reset();
    slides.clear();
}
