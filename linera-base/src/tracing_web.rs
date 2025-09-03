// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module provides unified handling for tracing subscribers within Linera binaries.

use tracing_subscriber::{
    prelude::__tracing_subscriber_SubscriberExt as _, util::SubscriberInitExt as _,
};

/// Initializes tracing for the browser, sending messages to the developer console and
/// span events to the [Performance
/// API](https://developer.mozilla.org/en-US/docs/Web/API/Performance).
pub fn init() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .without_time()
            // .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW | tracing_subscriber::fmt::format::FmtSpan::CLOSE)
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
                .with_writer(tracing_web::MakeWebConsoleWriter::new()),
        )
        .with(
            tracing_web::performance_layer()
                .with_details_from_fields(tracing_subscriber::fmt::format::Pretty::default()),
        )
        .init();
}
