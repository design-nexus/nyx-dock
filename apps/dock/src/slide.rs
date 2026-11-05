//! The dock's slide out of sight and back: the bar moves off the screen edge, fading
//! as it goes, inside a window that keeps its size, so the layer surface isn't resized
//! while it moves.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};
use nyx_dock_core::config::Position;
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Slide {
        pub child: RefCell<Option<gtk::Widget>>,
        pub position: Cell<Position>,
        /// 0: up; 1: out of sight.
        pub offset: Cell<f64>,
        /// Where an animation is heading.
        pub target: Cell<f64>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Slide {
        const NAME: &'static str = "NyxDockSlide";
        type Type = super::Slide;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Slide {
        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Slide {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.child.borrow().as_ref() {
                Some(c) => {
                    let (min, nat, _, _) = c.measure(orientation, for_size);
                    (min, nat, -1, -1)
                }
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.child.borrow().clone() else { return };
            let (dx, dy) = self.obj().translation_for(width, height);
            let t = gsk::Transform::new().translate(&graphene::Point::new(dx as f32, dy as f32));
            child.allocate(width, height, baseline, Some(t));
        }
    }
}

glib::wrapper! {
    pub struct Slide(ObjectSubclass<imp::Slide>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Slide {
    pub fn new(child: &impl IsA<gtk::Widget>, position: Position, hidden: bool) -> Self {
        let slide: Slide = glib::Object::new();
        child.set_parent(&slide);
        let imp = slide.imp();
        *imp.child.borrow_mut() = Some(child.clone().upcast());
        imp.position.set(position);
        slide.apply(if hidden { 1.0 } else { 0.0 });
        slide
    }

    pub fn offset(&self) -> f64 {
        self.imp().offset.get()
    }

    /// Where it's heading: the offset an animation is moving to, else the offset.
    pub fn target(&self) -> f64 {
        if self.imp().tick.borrow().is_some() { self.imp().target.get() } else { self.offset() }
    }

    /// How far the bar is moved now, in pixels.
    pub fn translation(&self) -> (f64, f64) {
        self.translation_for(self.width(), self.height())
    }

    fn translation_for(&self, width: i32, height: i32) -> (f64, f64) {
        let o = self.imp().offset.get();
        match self.imp().position.get() {
            Position::Bottom => (0.0, o * height as f64),
            Position::Top => (0.0, -o * height as f64),
            Position::Left => (-o * width as f64, 0.0),
            Position::Right => (o * width as f64, 0.0),
        }
    }

    /// Jump to `offset`, stopping any animation.
    pub fn set_offset(&self, offset: f64) {
        self.stop();
        self.apply(offset);
    }

    /// Move to `target` over `ms`, then run `done`. Hiding speeds up as it goes; coming
    /// back slows down as it arrives.
    pub fn animate_to(&self, target: f64, ms: u32, done: impl FnOnce() + 'static) {
        self.stop();
        let from = self.offset();
        if (from - target).abs() < 0.001 || ms == 0 || !self.is_mapped() {
            self.apply(target);
            done();
            return;
        }
        self.imp().target.set(target);
        let hiding = target > from;
        let start: Cell<Option<i64>> = Cell::new(None);
        let done = RefCell::new(Some(done));
        let id = self.add_tick_callback(move |slide, clock| {
            let now = clock.frame_time();
            let t0 = start.get().unwrap_or(now);
            start.set(Some(t0));
            let t = ((now - t0) as f64 / (f64::from(ms) * 1000.0)).min(1.0);
            let eased = if hiding { t * t } else { 1.0 - (1.0 - t).powi(3) };
            slide.apply(from + (target - from) * eased);
            if t < 1.0 {
                return glib::ControlFlow::Continue;
            }
            slide.imp().tick.take();
            if let Some(done) = done.borrow_mut().take() {
                done();
            }
            glib::ControlFlow::Break
        });
        *self.imp().tick.borrow_mut() = Some(id);
    }

    fn stop(&self) {
        if let Some(id) = self.imp().tick.take() {
            id.remove();
        }
    }

    fn apply(&self, offset: f64) {
        let offset = offset.clamp(0.0, 1.0);
        self.imp().offset.set(offset);
        if let Some(child) = self.imp().child.borrow().as_ref() {
            // Fades over the last part of the way out.
            child.set_opacity(((1.0 - offset) / 0.4).min(1.0));
        }
        self.queue_allocate();
    }
}
