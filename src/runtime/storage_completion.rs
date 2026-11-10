// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use crate::CacheError;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

struct State<T> {
    result: Option<Result<T, CacheError>>,
    closed: bool,
    abandoned: bool,
    waker: Option<Waker>,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

impl<T> Shared<T> {
    fn finish(&self, result: Option<Result<T, CacheError>>) {
        let waker = {
            let mut state = self.state.lock().expect("storage completion lock poisoned");
            state.closed = true;
            if !state.abandoned {
                state.result = result;
            }
            state.waker.take()
        };
        self.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

pub(super) struct CompletionSender<T>(Option<Arc<Shared<T>>>);
pub(super) struct CompletionReceiver<T>(Arc<Shared<T>>);

pub(super) fn channel<T>() -> (CompletionSender<T>, CompletionReceiver<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            result: None,
            closed: false,
            abandoned: false,
            waker: None,
        }),
        ready: Condvar::new(),
    });
    (
        CompletionSender(Some(Arc::clone(&shared))),
        CompletionReceiver(shared),
    )
}

impl<T> CompletionSender<T> {
    pub(super) fn send(mut self, result: Result<T, CacheError>) {
        self.0
            .take()
            .expect("completion sender used once")
            .finish(Some(result));
    }
}

impl<T> Drop for CompletionSender<T> {
    fn drop(&mut self) {
        if let Some(shared) = self.0.take() {
            shared.finish(None);
        }
    }
}

impl<T> CompletionReceiver<T> {
    pub(super) fn wait(self) -> Result<T, CacheError> {
        let state = self
            .0
            .state
            .lock()
            .expect("storage completion lock poisoned");
        let mut state = self
            .0
            .ready
            .wait_while(state, |state| !state.closed)
            .expect("storage completion lock poisoned");
        let result = state.result.take().unwrap_or(Err(CacheError::Stopped));
        drop(state);
        result
    }
}

impl<T> Future for CompletionReceiver<T> {
    type Output = Result<T, CacheError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self
            .0
            .state
            .lock()
            .expect("storage completion lock poisoned");
        if state.closed {
            return Poll::Ready(state.result.take().unwrap_or(Err(CacheError::Stopped)));
        }
        if !state
            .waker
            .as_ref()
            .is_some_and(|waker| waker.will_wake(cx.waker()))
        {
            let previous = state.waker.replace(cx.waker().clone());
            drop(state);
            drop(previous);
        }
        Poll::Pending
    }
}

impl<T> Drop for CompletionReceiver<T> {
    fn drop(&mut self) {
        let retired = {
            let mut state = self
                .0
                .state
                .lock()
                .expect("storage completion lock poisoned");
            state.abandoned = true;
            (state.result.take(), state.waker.take())
        };
        drop(retired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_sender_completes_with_stopped() {
        let (sender, receiver) = channel::<()>();
        drop(sender);
        assert!(matches!(receiver.wait(), Err(CacheError::Stopped)));
    }

    #[test]
    fn result_ready_before_poll_is_preserved() {
        let (sender, mut receiver) = channel();
        sender.send(Ok(17));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            Pin::new(&mut receiver).poll(&mut cx),
            Poll::Ready(Ok(17))
        ));
    }
}
