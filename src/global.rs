//! The exception context that can be use as a `static` variable based on thread-local.

extern crate std;

use std::thread::LocalKey;

/// `ExceptionContext` provides the context for throwing and catching exception.
///
/// This context wraps over a thread-local [crate::unsync::ExceptionContext],
/// and you can store it as a static variable, so that you don't need to pass the
/// context through argument.
///
/// Example:
/// ```
/// use asynx::global::ExceptionContext;
///
/// thread_local! {static EXCEPTION_LOCAL: asynx::unsync::ExceptionContext<String> = Default::default(); }
/// pub static CTX: ExceptionContext<String> = ExceptionContext::new(&EXCEPTION_LOCAL);
///
/// async fn perform(success: bool) -> String {
///     if success {
///         "success".to_string()
///     } else {
///         CTX.throw("failed".to_string()).await
///     }
/// }
///
/// tokio_test::block_on(async {
///     let r = CTX
///         .catch(async {
///             assert_eq!("success".to_string(), perform(true).await);
///             perform(false).await;
///             unreachable!() // The previous statement throws an exception.
///         })
///         .await;
///     assert_eq!(Err("failed".to_string()), r)
/// });
/// ```
pub struct ExceptionContext<E: 'static> {
    inner: &'static LocalKey<crate::unsync::ExceptionContext<E>>,
}

impl<E: 'static> ExceptionContext<E> {
    /// Create a new exception context from a thread-local unsync context.
    pub const fn new(
        local: &'static LocalKey<crate::unsync::ExceptionContext<E>>,
    ) -> ExceptionContext<E> {
        Self { inner: local }
    }

    /// Throws an exception. You should always `await` the result.
    ///
    /// Example:
    ///
    /// ```rust
    /// use asynx::global::ExceptionContext;
    ///
    /// thread_local! {static EXCEPTION_LOCAL: asynx::unsync::ExceptionContext<String> = Default::default(); }
    /// static CTX: ExceptionContext<String> = ExceptionContext::new(&EXCEPTION_LOCAL);
    ///
    /// tokio_test::block_on(async {
    ///     let r = CTX.catch(async {
    ///         CTX.throw("failed".to_string()).await;
    ///         unreachable!()
    ///     }).await;
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    pub async fn throw(&self, e: E) -> ! {
        self.inner.with(|ctx| {
            crate::unsync::set_exception(ctx, e);
        });
        core::future::pending().await
    }

    /// Creates the wrapper future that catches the exception.
    ///
    /// Example:
    ///
    /// ```
    /// use asynx::global::ExceptionContext;
    ///
    /// thread_local! {static EXCEPTION_LOCAL: asynx::unsync::ExceptionContext<String> = Default::default(); }
    /// static CTX: ExceptionContext<String> = ExceptionContext::new(&EXCEPTION_LOCAL);
    ///
    /// tokio_test::block_on(async {
    ///     let r = CTX.catch(async {
    ///         "success".to_string()
    ///     }).await;
    ///     assert_eq!(Ok("success".to_string()), r);
    ///
    ///     let r = CTX.catch(async {
    ///         CTX.throw("failed".to_string()).await;
    ///         unreachable!()
    ///     }).await;
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    ///
    /// Like the unsync version, the `catch` can be called multiple times.
    ///
    /// ```
    /// use asynx::global::ExceptionContext;
    ///
    /// thread_local! {static EXCEPTION_LOCAL: asynx::unsync::ExceptionContext<i32> = Default::default(); }
    /// static CTX: ExceptionContext<i32> = ExceptionContext::new(&EXCEPTION_LOCAL);
    ///
    /// tokio_test::block_on(async {
    ///     let r = CTX.catch(async {
    ///         let r = CTX.catch(async {
    ///             CTX.throw(1).await
    ///         }).await;
    ///         assert_eq!(Err(1), r);
    ///             
    ///         let r = CTX.catch(async {
    ///             CTX.throw(2).await
    ///         }).await;
    ///         assert_eq!(Err(2), r);
    ///
    ///         CTX.throw(3).await;
    ///     }).await;
    ///     assert_eq!(Err(3), r)
    /// })
    /// ```
    pub fn catch<Fu: core::future::Future>(&self, f: Fu) -> Catching<E, Fu> {
        Catching {
            ctx: self.inner,
            future: f,
        }
    }
}

pin_project_lite::pin_project! {
    /// A wrapper future that catches the exception for thread-local context.
    ///
    /// It outputs a result with the exception as error.
    pub struct Catching<E: 'static, F> {
        ctx: &'static LocalKey<crate::unsync::ExceptionContext<E>>,
        #[pin]
        future: F,
    }
}

impl<E: 'static, F: core::future::Future> core::future::Future for Catching<E, F> {
    type Output = Result<F::Output, E>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();
        this.ctx
            .with(|ctx| crate::unsync::poll_catching(ctx, this.future, cx))
    }
}
