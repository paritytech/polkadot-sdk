// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Reusable JAM zombienet harness.
//!
//! Provides the network, collator, genesis, RPC and assertion helpers that let
//! another crate drive a real JAM network.  The harness is unconditionally
//! compiled — the `jam-ci` feature gates only the integration-test binary in
//! `tests/`, not this library.

pub mod chain_spec;
pub mod collators;
pub mod env;
pub mod genesis;
pub mod genesis_build;
pub mod harness;
pub mod network;
pub mod rpc;
