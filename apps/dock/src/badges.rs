//! Badges and progress from apps, through the Unity launcher API that most Linux apps
//! and toolkits already speak: the `com.canonical.Unity.LauncherEntry.Update(s app_uri,
//! a{sv} properties)` signal, with `count`, `count-visible`, `progress`,
//! `progress-visible` and `urgent`.

use gtk::gio;
use gtk::glib;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Badge {
    pub count: Option<i64>,
    pub progress: Option<f64>,
    pub urgent: bool,
}

#[derive(Debug, Default)]
struct Raw {
    count: i64,
    count_visible: bool,
    progress: f64,
    progress_visible: bool,
    urgent: bool,
}

impl Raw {
    fn badge(&self) -> Badge {
        Badge {
            count: (self.count_visible && self.count > 0).then_some(self.count),
            progress: self.progress_visible.then_some(self.progress.clamp(0.0, 1.0)),
            urgent: self.urgent,
        }
    }
}

/// `application://firefox.desktop` → `firefox`.
fn app_id(uri: &str) -> &str {
    let id = uri.strip_prefix("application://").unwrap_or(uri);
    id.strip_suffix(".desktop").unwrap_or(id)
}

/// Listen for updates; `on_change(app_id, badge)` runs on the main thread.
pub fn watch(on_change: impl Fn(&str, &Badge) + 'static) -> Option<gio::SignalSubscription> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let state: std::rc::Rc<std::cell::RefCell<HashMap<String, Raw>>> = Default::default();
    Some(bus.subscribe_to_signal(
        None,
        Some("com.canonical.Unity.LauncherEntry"),
        Some("Update"),
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some((uri, props)) = signal.parameters.get::<(String, glib::VariantDict)>() else { return };
            let id = app_id(&uri).to_string();
            let mut all = state.borrow_mut();
            let raw = all.entry(id.clone()).or_default();
            if let Some(v) = props.lookup_value("count", None) {
                raw.count = v.get::<i64>().or_else(|| v.get::<i32>().map(i64::from)).unwrap_or(raw.count);
            }
            if let Some(v) = props.lookup_value("count-visible", None).and_then(|v| v.get::<bool>()) {
                raw.count_visible = v;
            }
            if let Some(v) = props.lookup_value("progress", None).and_then(|v| v.get::<f64>()) {
                raw.progress = v;
            }
            if let Some(v) = props.lookup_value("progress-visible", None).and_then(|v| v.get::<bool>()) {
                raw.progress_visible = v;
            }
            if let Some(v) = props.lookup_value("urgent", None).and_then(|v| v.get::<bool>()) {
                raw.urgent = v;
            }
            let badge = raw.badge();
            drop(all);
            on_change(&id, &badge);
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_to_id() {
        assert_eq!(app_id("application://org.telegram.desktop.desktop"), "org.telegram.desktop");
        assert_eq!(app_id("application://firefox.desktop"), "firefox");
    }

    #[test]
    fn hidden_values_show_nothing() {
        let r = Raw { count: 3, count_visible: false, progress: 0.4, progress_visible: true, urgent: false };
        assert_eq!(r.badge(), Badge { count: None, progress: Some(0.4), urgent: false });
        let r = Raw { count: 0, count_visible: true, ..Default::default() };
        assert_eq!(r.badge().count, None, "a zero count shows no badge");
    }
}
