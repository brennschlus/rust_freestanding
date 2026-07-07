use conquer_once::spin::OnceCell;
use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::Stream;
use futures_util::task::AtomicWaker;

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// Read every byte the i8042 controller has buffered and queue the
/// keyboard ones. Draining completely matters: leaving a byte unread
/// keeps the controller's interrupt line high, and the edge-triggered
/// PIC then never sees another keyboard interrupt -- input dies. Called
/// from the keyboard interrupt and, as a lost-edge safety net, from the
/// timer tick.
///
/// Must not block or allocate: it only pushes into a pre-allocated
/// lock-free queue and wakes the stream task.
pub(crate) fn drain_controller() {
    use x86_64::instructions::port::Port;

    let mut status_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);
    loop {
        let status = unsafe { status_port.read() };
        if status & 0x01 == 0 {
            break; // output buffer empty
        }
        let byte = unsafe { data_port.read() };
        if status & 0x20 != 0 {
            continue; // mouse byte: discard, we only speak keyboard
        }
        if let Ok(queue) = SCANCODE_QUEUE.try_get() {
            if queue.push(byte).is_err() {
                log::warn!("scancode queue full; dropping keyboard input");
            } else {
                WAKER.wake();
            }
        }
    }
}

pub struct ScancodeStream {
    _private: (), // prevent construction without new()
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE.try_get().expect("queue not initialized");

        // fast path: skip the waker dance if a scancode is already there
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(cx.waker());
        // check again: an interrupt could have pushed a scancode between
        // the first pop and registering the waker
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

