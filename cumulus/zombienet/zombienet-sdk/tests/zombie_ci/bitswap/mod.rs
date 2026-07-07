// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet e2e coverage of the `bitswap_unstable_*` JSON-RPC namespace, plus the generator
//! that produces the bulletin DB snapshots those tests consume.
//!
//! Structure mirrors `full_node_warp_sync/`, except both tests are always compiled and gated only
//! by `#[ignore]` (they need zombienet binaries) rather than a cargo feature:
//! - [`payloads`] — the deterministic bulletin payloads / CIDs, shared by the consumer test and the
//!   generator so the two can never drift.
//! - `e2e` — the consumer test (`bitswap_unstable_e2e`).
//! - `generate_snapshot` — the snapshot generator (`bitswap_generate_snapshot`).

mod common;
mod payloads;

mod e2e;
mod generate_snapshot;
