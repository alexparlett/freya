use std::time::{
    Duration,
    Instant,
};

use freya_core::{
    notify::Notify,
    prelude::*,
};

#[derive(Clone, Copy, PartialEq)]
pub struct Timeout {
    elapsed: State<bool>,
    instant: State<Instant>,
    reset: State<Notify>,
}

impl Timeout {
    /// You most likely want to use [use_timeout].
    pub fn create(duration: Duration) -> Self {
        let mut elapsed = State::create(false);
        let instant = State::create(Instant::now());
        let reset = State::create(Notify::new());

        // Sleeps until the latest reset's deadline, then until the next reset, so an idle
        // timeout wakes nothing.
        let notify = reset.peek().clone();
        spawn(async move {
            loop {
                loop {
                    let remaining = duration.saturating_sub(instant.peek().elapsed());
                    if remaining.is_zero() {
                        break;
                    }
                    timer(remaining).await;
                }
                elapsed.set_if_modified(true);
                notify.notified().await;
            }
        });

        Timeout {
            elapsed,
            instant,
            reset,
        }
    }

    /// Check if the timeout has passed its specified [Duration].
    pub fn elapsed(&self) -> bool {
        (self.elapsed)()
    }

    /// Reset the timer.
    pub fn reset(&mut self) {
        self.instant.set_if_modified(Instant::now());
        self.elapsed.set_if_modified(false);
        self.reset.peek().notify();
    }
}

/// Create a timeout with a given [Duration].
/// This is useful to dynamically render a UI if only the timeout has not elapsed yet.
///
/// You can reset it by calling [Timeout::reset],
/// use [Timeout::elapsed] to check if it has timed out or not.
pub fn use_timeout(duration: impl FnOnce() -> Duration) -> Timeout {
    use_hook(|| Timeout::create(duration()))
}
