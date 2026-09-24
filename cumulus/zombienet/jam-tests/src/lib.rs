// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Reusable JAM zombienet library.
//!
//! The pure building blocks a test crate composes into a JAM run: the para description, the
//! genesis, the chain spec, the RPC clients, the control lane and the accumulated-head read. The
//! network orchestration itself lives in the consuming test crate (`cumulus-zombienet-sdk-tests`,
//! `tests/jam/mod.rs`), which spawns the JAM network through zombienet-sdk 0.5.0.

pub mod chain_spec;
pub mod control;
pub mod env;
pub mod genesis;
pub mod genesis_build;
pub mod network;
pub mod para;
pub mod para_head;
pub mod proxy;
pub mod rpc;
