//! Shared plumbing between baseview and the platform AccessKit adapters.

#![expect(dead_code, reason = "used by the platform adapters in subsequent tasks")]

use crate::AccessibilityEvent;
use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A queue of accessibility events waiting to be delivered to the window handler.
///
/// AccessKit calls back into us at moments where re-entering user code is unsafe: activation
/// happens inside the platform's own message handling, and on Windows actions may arrive on a UI
/// Automation RPC thread. Callbacks therefore only push here and wake the window thread, which
/// drains the queue and dispatches through the normal event path.
#[derive(Clone)]
pub(crate) struct AccessibilityQueue {
    inner: Arc<Inner>,
}

struct Inner {
    events: Mutex<VecDeque<AccessibilityEvent>>,
    wake: Box<dyn Fn() + Send + Sync + 'static>,
}

impl AccessibilityQueue {
    /// Creates a queue that calls `wake` whenever an event is pushed.
    ///
    /// `wake` must be callable from any thread, and must cause [`drain`](Self::drain) to be called
    /// on the window thread soon afterwards.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(Inner {
                events: Mutex::new(VecDeque::new()),
                wake: Box::new(wake),
            }),
        }
    }

    fn push(&self, event: AccessibilityEvent) {
        {
            let Ok(mut events) = self.inner.events.lock() else { return };
            events.push_back(event);
        }

        (self.inner.wake)();
    }

    /// Takes every queued event. Must be called on the window thread.
    pub fn drain(&self) -> Vec<AccessibilityEvent> {
        let Ok(mut events) = self.inner.events.lock() else { return Vec::new() };
        events.drain(..).collect()
    }

    /// An [`ActivationHandler`] that defers activation to the window thread.
    pub fn activation_handler(&self) -> QueuedActivationHandler {
        QueuedActivationHandler { queue: self.clone() }
    }

    /// An [`ActionHandler`] that defers action requests to the window thread.
    pub fn action_handler(&self) -> QueuedActionHandler {
        QueuedActionHandler { queue: self.clone() }
    }
}

pub(crate) struct QueuedActivationHandler {
    queue: AccessibilityQueue,
}

impl ActivationHandler for QueuedActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // We can never answer synchronously: the tree is a product of the handler rendering a
        // frame. Returning `None` leaves the adapter in its placeholder state until the handler
        // pushes a full tree through `update_accessibility_tree`.
        self.queue.push(AccessibilityEvent::Enabled);
        None
    }
}

pub(crate) struct QueuedActionHandler {
    queue: AccessibilityQueue,
}

impl ActionHandler for QueuedActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        self.queue.push(AccessibilityEvent::ActionRequested(request));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    fn assert_clone<T: Clone>() {}

    #[test]
    fn test_queue_traits() {
        assert_send::<AccessibilityQueue>();
        assert_sync::<AccessibilityQueue>();
        assert_clone::<AccessibilityQueue>();
        assert_send::<QueuedActionHandler>();
    }

    #[test]
    fn test_queue_flow() {
        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_clone = Arc::clone(&wake_count);
        let queue = AccessibilityQueue::new(move || {
            wake_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let mut activation = queue.activation_handler();
        assert!(activation.request_initial_tree().is_none());
        assert_eq!(wake_count.load(Ordering::SeqCst), 1);

        let mut action = queue.action_handler();
        let request = ActionRequest {
            action: accesskit::Action::Focus,
            target_tree: accesskit::TreeId(accesskit::Uuid::nil()),
            target_node: accesskit::NodeId(0),
            data: None,
        };
        action.do_action(request);
        assert_eq!(wake_count.load(Ordering::SeqCst), 2);

        let events = queue.drain();
        assert_eq!(events.len(), 2);
        assert!(matches!(events.first(), Some(AccessibilityEvent::Enabled)));
        assert!(matches!(events.get(1), Some(AccessibilityEvent::ActionRequested(_))));

        let empty = queue.drain();
        assert!(empty.is_empty());
    }
}
