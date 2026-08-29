// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the JAM collator.
//!
//! Every test spins up its own JAM network with zombienet-sdk, registers the parasim service on
//! it, and runs a set of `polkadot-omni-node` collators against it, so nothing outside the test's
//! own work dir is needed or touched.

mod chain_spec;
mod collator_progress;
mod collators;
mod demo;
mod env;
mod harness;
mod network;
mod rpc;
