// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*!
Abstractions over time that can be used natively or on the Web.
 */

cfg_if::cfg_if! {
    if #[cfg(web)] {
        pub use web_time::*;
        /// DOCS
        pub mod timer {
            /// DOCS
            pub async fn sleep(duration: super::Duration) {
                tracing::debug!("sleeping for {duration:?}");
                wasmtimer::tokio::sleep(duration).await;
                tracing::debug!("sleep for {duration:?} completed");
            }

            /// DOCS
            pub async fn timeout<F: std::future::Future>(duration: super::Duration, future: F) -> Result<F::Output, wasmtimer::tokio::error::Elapsed> {
                tracing::debug!("timing out future after {duration:?}");
                let result = wasmtimer::tokio::timeout(duration, future).await;
                tracing::debug!(?duration, timed_out=result.is_err(), "future completed or timed out");
                result
            }
        }
    } else {
        pub use std::time::*;
        pub use tokio::time as timer;
    }
}
