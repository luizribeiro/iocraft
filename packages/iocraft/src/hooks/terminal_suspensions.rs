use futures::{future::poll_fn, task::AtomicWaker};
use std::{sync::Mutex, task::Poll};

type Callback = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub(crate) struct TerminalSuspensions {
    pending: Mutex<Option<Callback>>,
    waker: AtomicWaker,
}

impl TerminalSuspensions {
    pub(crate) async fn next(&self) -> Callback {
        poll_fn(|cx| {
            self.waker.register(cx.waker());
            match self.pending.lock().unwrap().take() {
                Some(callback) => Poll::Ready(callback),
                None => Poll::Pending,
            }
        })
        .await
    }

    #[cfg(test)]
    pub(crate) fn request(&self, callback: impl FnOnce() + Send + 'static) -> bool {
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.is_some() {
                return false;
            }
            *pending = Some(Box::new(callback));
        }
        self.waker.wake();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_request_is_taken_once() {
        let queue = TerminalSuspensions::default();
        let mut request = Box::pin(queue.next());
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        assert!(std::future::Future::poll(request.as_mut(), &mut cx).is_pending());

        assert!(queue.request(|| {}));
        assert!(!queue.request(|| panic!("duplicate callback was queued")));
        let Poll::Ready(callback) = std::future::Future::poll(request.as_mut(), &mut cx) else {
            panic!("missing callback");
        };
        callback();
        assert!(queue.request(|| {}));
    }
}
