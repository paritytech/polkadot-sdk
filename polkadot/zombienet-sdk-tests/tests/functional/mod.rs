// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

mod approval_voting_coalescing;
mod approved_peer_mixed_validators;
mod async_backing_6_seconds_rate;
mod beefy_and_mmr;
mod chunk_fetching_network_compatibility;
mod coretime_shared_core;
mod dispute_freshly_finalized;
mod dispute_old_finalized;
mod duplicate_collations;
mod parachains_max_tranche0;
mod shared_core_idle_parachain;
mod spam_statement_distribution_requests;
mod sync_backing;
mod validator_disabling;

// Disable PVF test temporarily
// since depends on the below:
// https://github.com/paritytech/zombienet-sdk/pull/487
// https://github.com/paritytech/zombienet-sdk/pull/484
//mod parachains_pvf;
