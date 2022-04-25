#![no_std]

//! Library that helps you to simulate exception without `panic` in async Rust.
//!
//! There is an unsync version [unsync::ExceptionContext] and a sync version [sync::ExceptionContext].
//!
//! You can also define the context as a static variable so that you don't
//! have to pass them through function argument, using [global::ExceptionContext].
//!
//! Check [this blog](https://jason5lee.me/2022/03/11/rust-exception-async/) for the main idea.
//!
//! Example:
//!
//! ```
//! type ExcptCtx = asynx::unsync::ExceptionContext<String>;
//!
//! async fn perform(ctx: &ExcptCtx, success: bool) -> String {
//!     if success {
//!         "success".to_string()
//!     } else {
//!         ctx.throw("failed".to_string()).await
//!     }
//! }
//!
//! tokio_test::block_on(async {
//!     let r = ExcptCtx::new()
//!         .catch(|ctx| async move {
//!             assert_eq!("success".to_string(), perform(ctx, true).await);
//!             perform(ctx, false).await;
//!             unreachable!() // The previous statement throws an exception.
//!         })
//!         .await;
//!     assert_eq!(Err("failed".to_string()), r)
//! });
//! ```
#[cfg(feature = "global")]
pub mod global;
pub mod sync;
pub mod unsync;
