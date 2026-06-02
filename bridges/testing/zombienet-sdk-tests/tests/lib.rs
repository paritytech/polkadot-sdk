// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Zombienet-sdk based integration tests for the local Rococo <> Westend bridge.
//!
//! These are the Rust port of the legacy bash + `zndsl` tests that used to live under
//! `bridges/testing/tests/`. They spawn two relay-chain networks (Rococo and Westend), each with
//! a Bridge Hub and an Asset Hub parachain, drive the external `substrate-relay` binary as a set
//! of subprocesses and assert on-chain state via `subxt`.
//!
//! The tests are gated behind the `zombie-ci` feature so that a plain `cargo check` of the
//! workspace stays cheap and does not require the generated metadata files.

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/rococo-local.scale")]
pub mod rococo {}

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/westend-local.scale")]
pub mod westend {}

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/asset-hub-rococo-local.scale")]
pub mod asset_hub_rococo {}

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/asset-hub-westend-local.scale")]
pub mod asset_hub_westend {}

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/bridge-hub-rococo-local.scale")]
pub mod bridge_hub_rococo {}

#[cfg(feature = "zombie-ci")]
#[subxt::subxt(runtime_metadata_path = "metadata-files/bridge-hub-westend-local.scale")]
pub mod bridge_hub_westend {}

#[cfg(feature = "zombie-ci")]
mod environment;

#[cfg(feature = "zombie-ci")]
mod asset_transfer;
#[cfg(feature = "zombie-ci")]
mod free_headers;
