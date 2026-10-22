use super::*;

/// Waker slot for one [`StateChanges`] collector.
pub(super) struct ChangesObserver {
    pub(super) waker: Mutex<Option<Waker>>,
}

impl StateObserver for ChangesObserver {
    fn on_change(&self, _id: StateId) {
        // Take the waker: the stream re-registers on its next poll, and a change that lands while
        // the stream is being polled is seen by that poll's read.
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
}

/// The stream returned by [`State::changes`].
pub struct StateChanges<T> {
    pub(super) inner: Arc<StateInner<T>>,
    pub(super) observer: Arc<ChangesObserver>,
    /// Last value handed out; `None` until the seed has been emitted.
    pub(super) last: Option<T>,
}

// Nothing in here relies on pinning, so the handle is free to move.
impl<T> Unpin for StateChanges<T> {}

impl<T: Clone + PartialEq> futures_util::Stream for StateChanges<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        // Register BEFORE reading: a write that lands between the read and the registration would
        // otherwise never wake this task.
        *this.observer.waker.lock() = Some(cx.waker().clone());
        let value = this.inner.value.read().clone();
        if this.last.as_ref() != Some(&value) {
            this.last = Some(value.clone());
            Poll::Ready(Some(value))
        } else {
            Poll::Pending
        }
    }
}

impl<T> Drop for StateChanges<T> {
    fn drop(&mut self) {
        let observer: Arc<dyn StateObserver> = self.observer.clone();
        self.inner.signal.unsubscribe_observer(&observer);
    }
}

impl<T: 'static> State<T> {
    /// Legacy u32 identity (animation/public compatibility).
    pub fn id(&self) -> u32 {
        self.raw.id()
    }

    /// Return the u64-backed identity used by StateSignal and dependency graphs.
    /// This is the migration API for callers that must not rely on the legacy u32 ID.
    pub fn state_id(&self) -> StateId {
        self.raw.state_id()
    }

    /// Internal alias used by Composer dependency maps.
    #[allow(dead_code)] // the dependency maps ask one handle type; the alias is on them all
    pub(crate) fn signal_id(&self) -> StateId {
        self.raw.signal_id()
    }

}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
        }
    }
}

impl<T: Debug> Debug for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("id", &self.raw.id())
            .field("value", &*self.raw.inner.value.read())
            .finish()
    }
}

impl<T: Display> Display for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&*self.raw.inner.value.read(), f)
    }
}

impl<T: PartialEq> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        // Identity semantics: same signal means same state, even if a
        // concurrent write changed the value between the two reads.
        self.raw.state_id() == other.raw.state_id()
    }
}

impl<T> Eq for State<T> where T: Eq {}

