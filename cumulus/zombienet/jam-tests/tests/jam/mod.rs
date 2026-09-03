// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the JAM collator.
//!
//! Every test spins up its own JAM network with zombienet-sdk — from a genesis that already
//! carries the parasim service, the paras' AURA authorizers and their cores — and runs a set of
//! `polkadot-omni-node` collators against it, so nothing outside the test's own work dir is
//! needed or touched.

mod chain_spec;
mod collator_progress;
mod collators;
mod core_assignment;
mod demo;
mod env;
mod genesis;
mod harness;
mod network;
mod rpc;
