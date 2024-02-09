// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    cfg_aliases::cfg_aliases! {
        web: { all(target_arch = "wasm32", target_os = "unknown") },
        with_metrics: { all(not(web), feature = "metrics") },
        with_scylladb: { all(not(web), feature = "scylladb") },
        with_rocksdb: { all(not(web), feature = "rocksdb") },
        with_dynamodb: { all(not(web), feature = "dynamodb") },
    };
}
