//! Following a workspace swipe. Nyx Dock's Hyprland plugin (`hyprland-plugin/`) reports
//! how far a workspace slide has gone; the dock slides out or in with it.

use nyx_desktop::Desktop;

/// What the plugin reports.
#[derive(Debug, Clone, PartialEq)]
pub enum SwipeEvent {
    /// `from` (the workspace the slide started on) is `progress` (0 to 1) of the way
    /// out of view, and `to` as far in.
    Moving { monitor: String, from: i64, to: i64, progress: f64 },
    /// Nothing moves any more: `active` is in front.
    Done { monitor: String, active: i64 },
}

/// A line from Hyprland's event socket, if it's one of the plugin's.
pub fn parse(line: &str) -> Option<SwipeEvent> {
    let (name, data) = line.trim_end().split_once(">>")?;
    match name {
        "nyxswipe" => {
            let mut parts = data.rsplitn(4, ',');
            let progress = parts.next()?.parse::<f64>().ok()?;
            let to = parts.next()?.parse().ok()?;
            let from = parts.next()?.parse().ok()?;
            let monitor = parts.next()?.to_string();
            Some(SwipeEvent::Moving { monitor, from, to, progress: progress.clamp(0.0, 1.0) })
        }
        "nyxswipeend" => {
            let (monitor, active) = data.rsplit_once(',')?;
            Some(SwipeEvent::Done { monitor: monitor.to_string(), active: active.parse().ok()? })
        }
        _ => None,
    }
}

/// Nyx Settings' wrap-around: Hyprland's swipe can't wrap, so empty placeholder
/// workspaces either side of 1..N give it somewhere to slide, and arriving on one jumps
/// to the other end of the loop.
pub const LOOP_START: &str = "settings-loop-start";
pub const LOOP_END: i64 = 11;

/// Where a swipe from `from` to `to` on `monitor` lands: `to`, or the other end of the
/// loop when `to` is one of its placeholders.
pub fn resolve_loop(desktop: &Desktop, monitor: &str, from: i64, to: i64) -> i64 {
    let on: Vec<_> = desktop.workspaces.iter().filter(|w| w.monitor == monitor).collect();
    let Some(start) = on.iter().find(|w| w.name == LOOP_START).map(|w| w.id) else { return to };
    if !on.iter().any(|w| w.id == LOOP_END) {
        return to;
    }
    // The loop's workspaces are kept open from 1 up: the last is where they stop.
    let last = (1..LOOP_END).take_while(|id| on.iter().any(|w| w.id == *id)).last().unwrap_or(1);
    if to == LOOP_END {
        if from == 1 { last } else { 1 }
    } else if to == start {
        if from == last { 1 } else { last }
    } else {
        to
    }
}

/// How much of a swipe the dock takes to move: it's out of the way before the window
/// coming in reaches it, and comes back only once the one leaving has passed it.
const LEAD: f64 = 0.6;

/// How hidden the dock is (0 up, 1 out of sight) `progress` of the way from a
/// workspace that wants it `from_hidden` to one that wants it `to_hidden`.
pub fn offset(from_hidden: bool, to_hidden: bool, progress: f64) -> f64 {
    let p = progress.clamp(0.0, 1.0);
    match (from_hidden, to_hidden) {
        (false, true) => (p / LEAD).min(1.0),
        (true, false) => 1.0 - ((p - (1.0 - LEAD)) / LEAD).max(0.0),
        (hidden, _) => f64::from(u8::from(hidden)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_desktop::model::Workspace;

    fn ws(id: i64, name: &str) -> Workspace {
        Workspace { id, name: name.into(), monitor: "eDP-2".into(), windows: 0, has_fullscreen: false, tiled_layout: String::new() }
    }

    fn looped() -> Desktop {
        let mut d = Desktop { workspaces: (1..=5).map(|i| ws(i, &i.to_string())).collect(), ..Default::default() };
        d.workspaces.push(ws(-1337, LOOP_START));
        d.workspaces.push(ws(LOOP_END, "11"));
        d
    }

    #[test]
    fn parses_the_plugins_events() {
        assert_eq!(
            parse("nyxswipe>>eDP-2,1,2,0.250\n"),
            Some(SwipeEvent::Moving { monitor: "eDP-2".into(), from: 1, to: 2, progress: 0.25 })
        );
        assert_eq!(
            parse("nyxswipe>>eDP-2,5,-1337,1.2"),
            Some(SwipeEvent::Moving { monitor: "eDP-2".into(), from: 5, to: -1337, progress: 1.0 })
        );
        assert_eq!(parse("nyxswipeend>>eDP-2,3"), Some(SwipeEvent::Done { monitor: "eDP-2".into(), active: 3 }));
        assert_eq!(parse("workspace>>3"), None);
        assert_eq!(parse("nyxswipe>>eDP-2,x,2,0.5"), None);
    }

    #[test]
    fn empty_to_app_hides_with_the_swipe() {
        assert_eq!(offset(false, true, 0.0), 0.0);
        assert!((offset(false, true, 0.3) - 0.5).abs() < 1e-9);
        assert_eq!(offset(false, true, 0.6), 1.0, "gone before the window reaches it");
        assert_eq!(offset(false, true, 1.0), 1.0);
    }

    #[test]
    fn app_to_empty_shows_once_the_window_has_passed() {
        assert_eq!(offset(true, false, 0.0), 1.0);
        assert_eq!(offset(true, false, 0.4), 1.0);
        assert!((offset(true, false, 0.7) - 0.5).abs() < 1e-9);
        assert_eq!(offset(true, false, 1.0), 0.0);
    }

    #[test]
    fn app_to_app_never_shows() {
        for p in [0.0, 0.2, 0.5, 0.8, 1.0] {
            assert_eq!(offset(true, true, p), 1.0);
            assert_eq!(offset(false, false, p), 0.0);
        }
    }

    #[test]
    fn a_cancelled_swipe_comes_back() {
        // Progress rises, then falls back to 0: the dock ends where it started.
        let path: Vec<f64> = [0.0, 0.3, 0.5, 0.2, 0.0].iter().map(|p| offset(false, true, *p)).collect();
        assert_eq!(path.last(), Some(&0.0));
    }

    #[test]
    fn placeholders_stand_for_the_other_end() {
        let d = looped();
        assert_eq!(resolve_loop(&d, "eDP-2", 5, LOOP_END), 1);
        assert_eq!(resolve_loop(&d, "eDP-2", 1, LOOP_END), 5, "backing out of 1");
        assert_eq!(resolve_loop(&d, "eDP-2", 1, -1337), 5);
        assert_eq!(resolve_loop(&d, "eDP-2", 5, -1337), 1);
        assert_eq!(resolve_loop(&d, "eDP-2", 2, 3), 3);
        assert_eq!(resolve_loop(&d, "HDMI-1", 5, LOOP_END), LOOP_END, "another monitor");
    }

    #[test]
    fn no_loop_no_placeholders() {
        let mut d = looped();
        d.workspaces.retain(|w| w.name != LOOP_START);
        assert_eq!(resolve_loop(&d, "eDP-2", 5, LOOP_END), LOOP_END);
    }

    #[test]
    fn the_last_is_where_the_open_run_stops() {
        let mut d = looped();
        d.workspaces.push(ws(8, "8"));
        assert_eq!(resolve_loop(&d, "eDP-2", 1, LOOP_END), 5);
    }
}
