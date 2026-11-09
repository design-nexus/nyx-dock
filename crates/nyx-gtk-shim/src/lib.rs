//! Just enough of the `gtk` names for the GTK dock's GIO-only modules (launching, badges,
//! the swipe listener) to build here unchanged: GIO and GLib themselves, and a display
//! that has no launch context (GIO launches without one).

pub use gio;
pub use glib;

pub mod prelude {
    pub use gio::prelude::*;
}

pub mod gdk {
    pub struct Display;

    impl Display {
        #[allow(clippy::should_implement_trait)]
        pub fn default() -> Option<Display> {
            None
        }

        pub fn app_launch_context(&self) -> gio::AppLaunchContext {
            gio::AppLaunchContext::new()
        }
    }
}
