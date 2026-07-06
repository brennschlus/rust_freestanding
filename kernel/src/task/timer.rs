use crate::interrupts;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_util::task::AtomicWaker;

static SLEEP_WAKER: AtomicWaker = AtomicWaker::new();

/// Called from the timer interrupt on every tick.
pub(crate) fn on_tick() {
    SLEEP_WAKER.wake();
}

/// Sleep for roughly `ms` milliseconds. Granularity is one PIT tick
/// (~55 ms), rounded up.
///
/// Note: a single `AtomicWaker` supports only one concurrent sleeper,
/// which is fine while the shell is the only task that sleeps.
pub async fn sleep_ms(ms: u64) {
    let target = interrupts::ticks() + ms.div_ceil(55).max(1);
    Sleep { target }.await
}

struct Sleep {
    target: u64,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if interrupts::ticks() >= self.target {
            return Poll::Ready(());
        }
        SLEEP_WAKER.register(cx.waker());
        // re-check: a tick could have happened between the first check
        // and registering the waker
        if interrupts::ticks() >= self.target {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
