use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// From https://docs.rs/futures/latest/futures/macro.pending.html.
pub fn pending_once() -> PendingOnce {
    PendingOnce { is_ready: false }
}

pub struct PendingOnce {
    is_ready: bool,
}

impl Future for PendingOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        if self.is_ready {
            Poll::Ready(())
        } else {
            self.is_ready = true;
            Poll::Pending
        }
    }
}
