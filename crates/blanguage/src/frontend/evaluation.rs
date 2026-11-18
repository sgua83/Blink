//! The original World call stack. Each child finishes before its parent is
//! polled again; the actual function future keeps its own values and borrows.
use std::{
    cell::RefCell,
    future::Future,
    marker::PhantomPinned,
    mem,
    pin::{Pin, pin},
    ptr,
    task::{Context, Poll, Waker},
};

trait Frame {
    unsafe fn set_output(self: Pin<&mut Self>, output: *mut ());
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()>;
}

struct Invocation<F: Future> {
    future: F,
    output: *mut Option<F::Output>,
}

impl<F: Future> Frame for Invocation<F> {
    unsafe fn set_output(self: Pin<&mut Self>, output: *mut ()) {
        // Only call constructs this pairing: the destination is Option<F::Output>.
        unsafe { self.get_unchecked_mut() }.output = output.cast();
    }
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        // Invocation is allocated once. Neither the frame nor its future moves
        // until the original child is finished and the frame is dropped.
        let this = unsafe { self.get_unchecked_mut() };
        match unsafe { Pin::new_unchecked(&mut this.future) }.poll(context) {
            Poll::Ready(output) => {
                // The destination is in the pinned parent Call. LIFO execution
                // and LIFO destruction keep that parent alive through this child.
                unsafe { this.output.write(Some(output)) };
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Default)]
pub(super) struct Calls {
    frames: RefCell<Vec<Pin<Box<dyn Frame>>>>,
}

/// The actual child and its one typed return destination in its parent.
pub(crate) struct Call<'a, T> {
    calls: &'a Calls,
    child: Option<Pin<Box<dyn Frame + 'a>>>,
    output: Option<T>,
    _pinned: PhantomPinned,
}

impl<T> Future for Call<'_, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        // After the first poll the child holds the address of this output slot.
        // Call is !Unpin and no field holding a live future is moved.
        let this = unsafe { self.get_unchecked_mut() };
        if let Some(output) = this.output.take() {
            return Poll::Ready(output);
        }
        if let Some(mut child) = this.child.take() {
            unsafe { child.as_mut().set_output((&mut this.output as *mut Option<T>).cast()) };
            this.calls.push(child);
        }
        Poll::Pending
    }
}

impl Calls {
    /// The call must be awaited by the current original World invocation.
    /// It may not be independently polled, cancelled, or moved after polling.
    pub(super) unsafe fn call<'a, F: Future + 'a>(&'a self, future: F) -> Call<'a, F::Output> {
        Call {
            calls: self,
            child: Some(Box::pin(Invocation {
                future,
                output: ptr::null_mut(),
            })),
            output: None,
            _pinned: PhantomPinned,
        }
    }

    fn push<'a>(&self, frame: Pin<Box<dyn Frame + 'a>>) {
        // Only run polls these frames. A child borrows its pinned parent; run
        // completes or drops all children before any parent or root can end.
        // Erasing the borrow permits that one scoped stack, not escaped work.
        let frame =
            unsafe { mem::transmute::<Pin<Box<dyn Frame + 'a>>, Pin<Box<dyn Frame>>>(frame) };
        self.frames.borrow_mut().push(frame);
    }

    /// Run only original functions that yield through awaited calls on this
    /// stack. No nested run, external future, manual child polling, or select.
    pub(super) unsafe fn run<F: Future>(&self, future: F) -> F::Output {
        let mut root = pin!(unsafe { self.call(future) });
        // Declared after root: children are dropped first on every scope exit.
        let _scope = Scope(self);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = root.as_mut().poll(&mut context) {
                return output;
            }
            loop {
                let frame = {
                    let mut frames = self.frames.borrow_mut();
                    frames.last_mut().map(|frame| {
                        // Box addresses stay fixed when the stack grows. Release
                        // the Vec borrow before the child can append its own call.
                        unsafe { Pin::get_unchecked_mut(frame.as_mut()) as *mut dyn Frame }
                    })
                };
                let Some(frame) = frame else { break };
                match unsafe { Pin::new_unchecked(&mut *frame) }.poll(&mut context) {
                    Poll::Ready(()) => {
                        self.frames.borrow_mut().pop();
                    }
                    Poll::Pending => {}
                }
            }
        }
    }
}

struct Scope<'a>(&'a Calls);

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        // A suspended child can borrow any live parent. End them in the same
        // actual call order as normal completion, before the root is released.
        loop {
            let frame = { self.0.frames.borrow_mut().pop() };
            let Some(frame) = frame else { break };
            drop(frame);
        }
    }
}
