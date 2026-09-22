// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Runtime API for cross-parachain **source discovery** configuration.
//!
//! A receiver parachain records, per source [`ParaId`], how to reach that
//! source's collators over the relay-chain DHT — its genesis hash (and optional
//! fork id), set on-chain by governance (see `cumulus-pallet-source-discovery`).
//! The off-chain discovery client (`cumulus-client-source-discovery`) reads this
//! to resolve and maintain that source's peer set.
//!
//! Version-gated by design: a runtime that does not implement [`SourceDiscoveryApi`]
//! (or configures no sources) runs **no** cross-parachain discovery — identical to
//! a node without the feature.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use cumulus_primitives_core::ParaId;

/// How to reach a source parachain's collators: its 32-byte genesis hash and
/// optional fork id. The `/paranode` discovery response is verified against
/// these. `None` fork id is the common case.
pub type SourceInfo = ([u8; 32], Option<Vec<u8>>);

sp_api::decl_runtime_apis! {
	/// Per-source discovery configuration, exposed to the node's discovery client.
	pub trait SourceDiscoveryApi {
		/// The configured sources and how to reach each: `(source, (genesis,
		/// fork_id))`. An empty result means no cross-parachain discovery is
		/// configured — the discovery client then does nothing.
		fn source_discovery_info() -> Vec<(ParaId, SourceInfo)>;
	}
}
