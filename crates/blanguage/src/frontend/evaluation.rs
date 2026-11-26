//! B owns the active evaluation stack. Rust supplies each pinned future and its typed output.
use std::{
    cell::UnsafeCell,
    ffi::c_void,
    future::Future,
    marker::{PhantomData, PhantomPinned},
    mem,
    pin::Pin,
    ptr::{self, NonNull},
    task::{Context, Poll, Waker},
};

#[repr(C)]
struct Frame {
    // B links the first field. The remaining fields are private to this adapter.
    parent: *mut Frame,
    output: *mut (),
    poll: unsafe fn(*mut Frame, &mut Context<'_>) -> Poll<()>,
    release: unsafe fn(*mut Frame),
}

#[repr(C)]
struct Invocation<F> {
    frame: Frame,
    future: F,
}

unsafe fn poll<F: Future>(frame: *mut Frame, context: &mut Context<'_>) -> Poll<()> {
    let invocation = unsafe { &mut *frame.cast::<Invocation<F>>() };
    match unsafe { Pin::new_unchecked(&mut invocation.future) }.poll(context) {
        Poll::Ready(value) => {
            // The parent is pinned through this call, or run owns the root slot.
            unsafe { invocation.frame.output.cast::<Option<F::Output>>().write(Some(value)) };
            Poll::Ready(())
        }
        Poll::Pending => Poll::Pending,
    }
}

unsafe fn release<F>(frame: *mut Frame) {
    drop(unsafe { Box::from_raw(frame.cast::<Invocation<F>>()) });
}

unsafe extern "C-unwind" fn poll_frame(frame: *mut c_void, context: *mut c_void) -> u32 {
    let frame = frame.cast::<Frame>();
    u32::from(unsafe { ((*frame).poll)(frame, &mut *context.cast::<Context<'_>>()) }.is_ready())
}

unsafe extern "C-unwind" fn release_frame(frame: *mut c_void) {
    let frame = frame.cast::<Frame>();
    unsafe { ((*frame).release)(frame); }
}

unsafe extern "C" {
    #[link_name = "bootstrap.world_evaluation::enter"]
    fn enter(top: *mut *mut c_void, frame: *mut c_void);
}

unsafe extern "C-unwind" {
    #[link_name = "bootstrap.world_evaluation::drive"]
    fn drive(
        top: *mut *mut c_void,
        context: *mut c_void,
        poll: unsafe extern "C-unwind" fn(*mut c_void, *mut c_void) -> u32,
        release: unsafe extern "C-unwind" fn(*mut c_void),
    );

    #[link_name = "bootstrap.world_evaluation::drain"]
    fn drain(top: *mut *mut c_void, release: unsafe extern "C-unwind" fn(*mut c_void));
}

struct Pending<'a, T> {
    frame: NonNull<Frame>,
    _borrow: PhantomData<&'a mut T>,
}

impl<'a, T> Pending<'a, T> {
    fn new<F: Future<Output = T> + 'a>(future: F) -> Self {
        let invocation = Box::new(Invocation {
            frame: Frame {
                parent: ptr::null_mut(),
                output: ptr::null_mut(),
                poll: poll::<F>,
                release: release::<F>,
            },
            future,
        });
        Self {
            frame: NonNull::from(Box::leak(invocation)).cast(),
            _borrow: PhantomData,
        }
    }

    // The destination must stay at this address through completion or draining.
    unsafe fn enter(self, calls: &Calls, output: &mut Option<T>) {
        let frame = self.frame.as_ptr();
        unsafe {
            (*frame).output = ptr::from_mut(output).cast();
            enter(calls.top.get(), frame.cast());
        }
        mem::forget(self);
    }
}

impl<T> Drop for Pending<'_, T> {
    fn drop(&mut self) {
        unsafe { release_frame(self.frame.as_ptr().cast()); }
    }
}

#[derive(Default)]
pub(super) struct Calls {
    top: UnsafeCell<*mut c_void>,
}

/// The actual child and its one typed return destination in its parent.
pub(crate) struct Call<'a, T> {
    calls: &'a Calls,
    child: Option<Pending<'a, T>>,
    output: Option<T>,
    _pinned: PhantomPinned,
}

impl<T> Future for Call<'_, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        // When the child enters the stack, Call keeps its output pinned.
        let this = unsafe { self.get_unchecked_mut() };
        if let Some(output) = this.output.take() {
            return Poll::Ready(output);
        }
        if let Some(child) = this.child.take() {
            unsafe { child.enter(this.calls, &mut this.output); }
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
            child: Some(Pending::new(future)),
            output: None,
            _pinned: PhantomPinned,
        }
    }

    /// Run only original functions that yield through awaited calls on this
    /// stack. No nested run, external future, manual child polling, or select.
    pub(super) unsafe fn run<F: Future>(&self, future: F) -> F::Output {
        let mut output = None;
        let mut context = Context::from_waker(Waker::noop());
        let scope = Scope(self);
        unsafe {
            Pending::new(future).enter(self, &mut output);
            drive(self.top.get(), ptr::from_mut(&mut context).cast(), poll_frame, release_frame);
        }
        // B has drained the stack. During unwinding Scope instead drains it
        // before the root output slot goes out of scope.
        mem::forget(scope);
        output.unwrap()
    }
}

struct Scope<'a>(&'a Calls);

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        unsafe { drain(self.0.top.get(), release_frame); }
    }
}
