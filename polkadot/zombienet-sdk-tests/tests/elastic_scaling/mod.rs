// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#[subxt::subxt(runtime_metadata_path = "metadata-files/rococo-local.scale")]
pub mod rococo {}

mod basic_3cores;
mod doesnt_break_parachains;
mod helpers;
mod mixed_receipt_versions;
mod slot_based_3cores;
