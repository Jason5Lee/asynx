//! The sync implementation of exception context.

use core::sync::atomic::AtomicU8;

const NO_EXCEPTION: u8 = 0;
const THROWING: u8 = 1;
const THROWN: u8 = 2;
const MOVED: u8 = 3;

/// `ExceptionContext` provides the context for throwing and catching exception.
///
/// This exception context implements `Send`/`Sync`, but the main purpose is to make
/// the future return from an async function that uses this context
/// has `Send`/`Sync`. It is not designed (though safe) to be operated concurrently.
pub struct ExceptionContext<E> {
    status: core::sync::atomic::AtomicU8,
    exception: core::cell::UnsafeCell<core::mem::MaybeUninit<E>>,
}

// SAFETY: There are no methods that deal with `&E`, so `E` is not required to be `Sync`. You can
// imagine that `E` is wrapped in a [`SyncWrapper`], a type that doesn't allow creation of `&E`s
// and is therefore unconditionally Sync.
//
// https://docs.rs/sync_wrapper/0.1/sync_wrapper/struct.SyncWrapper.html
unsafe impl<E: Send> Send for ExceptionContext<E> {}
// `E` must be `Send` as this type allows a shared reference to it to take ownership of `E`.
unsafe impl<E: Send> Sync for ExceptionContext<E> {}

impl<E> Drop for ExceptionContext<E> {
    fn drop(&mut self) {
        if *self.status.get_mut() == THROWN {
            // SAFETY: when the status is `THROWN`, `exception` has an unmoved initialized value.
            let e = unsafe { self.exception.get().read().assume_init() };
            drop(e)
        }
    }
}

impl<E> Default for ExceptionContext<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> ExceptionContext<E> {
    /// Create a new exception context.
    pub const fn new() -> Self {
        Self {
            status: AtomicU8::new(0),
            exception: core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()),
        }
    }

    /// Throws an exception. You should always `await` the result.
    ///
    /// Example:
    ///
    /// ```rust
    /// tokio_test::block_on(async {
    ///     let r = tokio::spawn(async {
    ///         asynx::sync::ExceptionContext::<String>::new()
    ///             .catch(|ctx| async move {
    ///                 ctx.throw("failed".to_string()).await;
    ///                 unreachable!()
    ///             }).await
    ///      }).await.unwrap();
    ///
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    pub async fn throw(&self, exception: E) -> ! {
        if self
            .status
            .compare_exchange(
                NO_EXCEPTION,
                THROWING,
                // No specific ordering is required on success because it's not possible for
                // `exception` to be written to before this swap succeds; there are no Release
                // fences that occur after a write to `exception` that any Acquire fence here would
                // need to synchronize with.
                core::sync::atomic::Ordering::Relaxed,
                core::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            panic!("`throw` calls more than once")
        }
        // SAFETY: we compare-exchange from NO_EXCEPTION to THROWING,
        // and the status won't be `NO_EXCEPTION` again.
        // So the compare-exchange will only succeed once, so there is no concurrent write.
        // Also, all reads on `exception` are performed only after status being written `THROWN`.
        // This happens after the exception is written,
        // so there is no concurrent read.
        unsafe { (&mut *self.exception.get()).write(exception) };
        // Release is necessary to ensure that any (correctly Acquired) reads of `exception` that
        // happen after this point will be able to see our newly-written value.
        self.status
            .store(THROWN, core::sync::atomic::Ordering::Release);
        core::future::pending().await
    }

    /// Executes the function `f` providing the context, then returns a Future that
    /// catches the thrown exception.
    ///
    /// Example:
    ///
    /// ```rust
    /// tokio_test::block_on(async {
    ///     let r = tokio::spawn(async {
    ///         asynx::sync::ExceptionContext::<String>::new()
    ///             .catch(|_| async {
    ///                 "success".to_string()
    ///             }).await
    ///     }).await.unwrap();
    ///     assert_eq!(Ok("success".to_string()), r);
    ///
    ///     let r = tokio::spawn(async {
    ///         asynx::sync::ExceptionContext::<String>::new()
    ///             .catch(|ctx| async {
    ///                 ctx.throw("failed".to_string()).await;
    ///                 unreachable!()
    ///             }).await
    ///      }).await.unwrap();
    ///
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    ///
    /// Note that unlike the unsync version,
    /// you can only call `catch` once on each context. Calling multiple times causes
    /// panic. You need to create a context for each catching.
    ///
    /// ```
    /// use asynx::sync::ExceptionContext;
    ///
    /// tokio_test::block_on(async {
    ///     let r = ExceptionContext::<u32>::new()
    ///         .catch(|ctx| async {
    ///             let r = ExceptionContext::<u32>::new().catch(|ctx| async {
    ///                 ctx.throw(1).await
    ///             }).await;
    ///             assert_eq!(Err(1), r);
    ///             
    ///             let r = ExceptionContext::<u32>::new().catch(|ctx| async {
    ///                 ctx.throw(2).await
    ///             }).await;
    ///             assert_eq!(Err(2), r);
    ///
    ///             ctx.throw(3).await;
    ///         }).await;
    ///     assert_eq!(Err(3), r)
    /// })
    /// ```
    pub fn catch<'a, Fu: core::future::Future, F: Fn(&'a Self) -> Fu>(
        &'a self,
        f: F,
    ) -> Catching<'a, E, Fu> {
        Catching {
            ctx: self,
            future: f(self),
        }
    }

    fn try_take_exception(&self) -> Option<E> {
        if self
            .status
            .compare_exchange(
                THROWN,
                MOVED,
                // On success, Acquire is necessary to ensure that our read of `exception`
                // happens-after it is written to by `throw`.
                core::sync::atomic::Ordering::Acquire,
                // We don't read any shared state on failure so there's no need for any memory
                // orderings.
                core::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            // SAFETY: status is changed from THROWN to MOVED,
            // but writes on exception only happens after status changed
            // from NO_EXCEPTION to THROWING, so there is no concurrent write.
            //
            // Because the status was THROWN before this write, the `exception` has an initialized value.
            // We can move it out because after status becomes MOVED, the value won't be dropped by the context itself.
            Some(unsafe { self.exception.get().read().assume_init() })
        } else {
            None
        }
    }
}

pin_project_lite::pin_project! {
    /// A wrapper future that catches the exception.
    ///
    /// It outputs a result with the exception as error.
    pub struct Catching<'a, E, F> {
        ctx: &'a ExceptionContext<E>,
        #[pin]
        future: F,
    }
}

impl<'a, E, F: core::future::Future> core::future::Future for Catching<'a, E, F> {
    type Output = Result<F::Output, E>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();
        let p = this.future.poll(cx);
        if let Some(exception) = this.ctx.try_take_exception() {
            core::task::Poll::Ready(Err(exception))
        } else {
            p.map(Ok)
        }
    }
}
