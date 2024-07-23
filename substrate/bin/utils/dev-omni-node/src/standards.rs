// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Hardcoded assumptions of this omni-node.
//!
//! Consensus: This template uses [`sc-manual-seal`] consensus and therefore has no expectation of
//! the runtime having any consensus-related pallets. The block time of the node can easily be
//! adjusted by [`crate::cli::Cli::consensus`]
//!
//! RPC: This node exposes only [`substrate_frame_rpc_system`] as additional RPC endpoints from the
//! runtime.

use sp_runtime::{traits, MultiSignature};

/// The account id type that is expected to be used in `frame-system`.
pub type AccountId =
	<<MultiSignature as traits::Verify>::Signer as traits::IdentifyAccount>::AccountId;
/// The index type that is expected to be used in `frame-system`.
pub type Nonce = u32;
/// The block type that is expected to be used in `frame-system`.
pub type BlockNumber = u32;
/// The hash type that is expected to be used in `frame-system`.
pub type Hashing = sp_runtime::traits::BlakeTwo256;

/// The hash type that is expected to be used in the runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
/// The opaque block type that is expected to be used in the runtime.
pub type OpaqueBlock = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;
