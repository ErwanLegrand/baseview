//! Demonstrates AccessKit screen reader support.
//!
//! Run this and inspect the window with a screen reader (NVDA or Narrator on Windows, VoiceOver on
//! macOS). The window reports two buttons; activating either prints to the console.

use baseview::accesskit::{
    Action, ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate,
};
use baseview::{
    AccessibilityEvent, Event, EventStatus, HandlerError, Window, WindowContext, WindowHandler,
    WindowSettings, WindowSize,
};
use std::cell::Cell;

const WINDOW: NodeId = NodeId(0);
const BUTTON_ONE: NodeId = NodeId(1);
const BUTTON_TWO: NodeId = NodeId(2);

struct Handler {
    ctx: WindowContext,
    accessibility_enabled: Cell<bool>,
    tree_sent: Cell<bool>,
}

impl Handler {
    fn build_tree(&self) -> TreeUpdate {
        let mut window = Node::new(Role::Window);
        window.set_children(vec![BUTTON_ONE, BUTTON_TWO]);
        window.set_bounds(Rect { x0: 0.0, y0: 0.0, x1: 400.0, y1: 200.0 });

        let mut one = Node::new(Role::Button);
        one.set_label("Play");
        one.set_bounds(Rect { x0: 20.0, y0: 20.0, x1: 180.0, y1: 80.0 });
        one.add_action(Action::Click);
        one.add_action(Action::Focus);

        let mut two = Node::new(Role::Button);
        two.set_label("Stop");
        two.set_bounds(Rect { x0: 200.0, y0: 20.0, x1: 360.0, y1: 80.0 });
        two.add_action(Action::Click);
        two.add_action(Action::Focus);

        TreeUpdate {
            nodes: vec![(WINDOW, window), (BUTTON_ONE, one), (BUTTON_TWO, two)],
            tree: Some(Tree::new(WINDOW)),
            tree_id: TreeId::ROOT,
            focus: BUTTON_ONE,
        }
    }
}

impl WindowHandler for Handler {
    fn on_frame(&self) -> Result<(), HandlerError> {
        if self.accessibility_enabled.get() && !self.tree_sent.get() {
            self.ctx.update_accessibility_tree(self.build_tree());
            self.tree_sent.set(true);
        }

        Ok(())
    }

    fn resized(&self, _new_size: WindowSize) -> Result<(), HandlerError> {
        Ok(())
    }

    fn on_event(&self, event: Event) -> EventStatus {
        match event {
            Event::Accessibility(AccessibilityEvent::Enabled) => {
                println!("Assistive technology attached.");
                self.accessibility_enabled.set(true);
            }
            Event::Accessibility(AccessibilityEvent::ActionRequested(ActionRequest {
                action,
                target_node,
                ..
            })) => {
                let name = match target_node {
                    BUTTON_ONE => "Play",
                    BUTTON_TWO => "Stop",
                    _ => "window",
                };

                println!("{action:?} requested on {name}.");
            }
            _ => {}
        }

        EventStatus::Ignored
    }
}

fn main() {
    // SAFETY: this example is a standalone application, not a plugin.
    unsafe { baseview::assume_standalone_in_process() };
    let settings = WindowSettings::new().with_title("baseview accessibility");

    let window = Window::create(settings, |ctx: WindowContext| {
        Ok(Handler { ctx, accessibility_enabled: Cell::new(false), tree_sent: Cell::new(false) })
    })
    .expect("failed to create window");

    window.run_until_closed().expect("event loop failed");
}
