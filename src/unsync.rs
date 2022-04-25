//! The unsync implementation of exception context.

/// `ExceptionContext` provides the context for throwing and catching exception.
///
/// Note that it is neither `Send` nor `Sync`, using it in an async function
/// will make the return future neither `Send` nor `Sync`.
///
/// If you need `Send` and `Sync` future, either use the sync version under [crate::sync] module
/// or the global version under [crate::global] module.
pub struct ExceptionContext<E> {
    catch_cnt: core::cell::Cell<usize>,
    exception: core::cell::Cell<Option<E>>,
}

impl<E> Default for ExceptionContext<E> {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn set_exception<E>(ctx: &ExceptionContext<E>, exception: E) {
    ctx.exception.set(Some(exception))
}
impl<E> ExceptionContext<E> {
    /// Create a new exception context.
    pub const fn new() -> Self {
        Self {
            catch_cnt: core::cell::Cell::new(0),
            exception: core::cell::Cell::new(None),
        }
    }

    /// Throws an exception. You should always `await` the result.
    ///
    /// Example:
    ///
    /// ```rust
    /// use asynx::unsync::ExceptionContext;
    ///
    /// tokio_test::block_on(async {
    ///     let r = ExceptionContext::<String>::new()
    ///         .catch(|ctx| async {
    ///             ctx.throw("failed".to_string()).await;
    ///             unreachable!()
    ///         }).await;
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    pub async fn throw(&self, exception: E) -> ! {
        if self.catch_cnt.get() == 0 {
            panic!("`throw` called not inside of `catch`")
        }
        set_exception(self, exception);
        core::future::pending().await
    }

    /// Executes the function `f` providing the context, then returns a Future that
    /// catches the thrown exception.
    ///
    /// Example:
    ///
    /// ```
    /// use asynx::unsync::ExceptionContext;
    ///
    /// tokio_test::block_on(async {
    ///     let r = ExceptionContext::<String>::new()
    ///         .catch(|_| async {
    ///             "success".to_string()
    ///         }).await;
    ///     assert_eq!(Ok("success".to_string()), r);
    ///
    ///     let r = ExceptionContext::<String>::new()
    ///         .catch(|ctx| async {
    ///             ctx.throw("failed".to_string()).await;
    ///             unreachable!()
    ///         }).await;
    ///     assert_eq!(Err("failed".to_string()), r)
    /// })
    /// ```
    ///
    /// For [crate::unsync::ExceptionContext], you can use `catch` multiple times on
    /// the same context, like this.
    ///
    /// ```
    /// use asynx::unsync::ExceptionContext;
    ///
    /// tokio_test::block_on(async {
    ///     let r = ExceptionContext::<u32>::new()
    ///         .catch(|ctx| async {
    ///             let r = ctx.catch(|ctx| async {
    ///                 ctx.throw(1).await
    ///             }).await;
    ///             assert_eq!(Err(1), r);
    ///             
    ///             let r = ctx.catch(|ctx| async {
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

struct DecOnDrop<'a>(&'a core::cell::Cell<usize>);
impl<'a> Drop for DecOnDrop<'a> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

pub(crate) fn poll_catching<E, Fu: core::future::Future>(
    ctx: &ExceptionContext<E>,
    future: core::pin::Pin<&mut Fu>,
    cx: &mut core::task::Context<'_>,
) -> core::task::Poll<Result<Fu::Output, E>> {
    ctx.catch_cnt.set(ctx.catch_cnt.get() + 1);
    let _dec_on_exit = DecOnDrop(&ctx.catch_cnt);

    let p = core::future::Future::poll(future, cx);
    if let Some(exception) = ctx.exception.take() {
        core::task::Poll::Ready(Err(exception))
    } else {
        p.map(Ok)
    }
}
impl<'a, E, F: core::future::Future> core::future::Future for Catching<'a, E, F> {
    type Output = Result<F::Output, E>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();
        poll_catching(this.ctx, this.future, cx)
    }
}
