// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Running a parachain on a JAM network instead of a relay chain.
//!
//! The building blocks a test composes into a JAM run: the para description, the genesis, the
//! chain spec, the RPC clients, the control lane and the accumulated-head read. The network
//! orchestration lives here too, in [`spawn`]: it is the one place that touches zombienet-sdk's
//! `NetworkConfigBuilder`, and it hands back a running [`spawn::JamNetwork`] to drive and tear
//! down, and [`topology`] holds the networks that need more than a list of paras. [`JamConfig`] is
//! the subxt config the parachain's clients are typed with; [`tx`] submits extrinsics to a JAM
//! para and `throughput` counts its candidates in JAM blocks.

mod config;

pub mod chain_spec;
pub mod control;
pub mod coretime;
pub mod env;
pub mod genesis;
pub mod genesis_build;
pub mod network;
pub mod para;
pub mod para_head;
pub mod proxy;
pub mod rpc;
pub mod spawn;
pub(crate) mod throughput;
pub mod topology;
pub mod tx;

pub use config::{Digest, DigestItem, JamConfig, JamHeader};
