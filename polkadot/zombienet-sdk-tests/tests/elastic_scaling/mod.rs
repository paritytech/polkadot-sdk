// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#[subxt::subxt(runtime_metadata_path = "metadata-files/rococo-local.scale")]
pub mod rococo {}

mod helpers;
mod slot_based_3cores;
