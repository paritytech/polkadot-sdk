// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Reusable JAM zombienet library.
//!
//! The building blocks a test crate composes into a JAM run: the para description, the genesis,
//! the chain spec, the RPC clients, the control lane and the accumulated-head read. The network
//! orchestration lives here too, in [`spawn`]: it is the one place that touches zombienet-sdk's
//! `NetworkConfigBuilder`, and it hands back a running [`spawn::JamNetwork`] to drive and tear
//! down.

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
pub mod spawn;
