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

//! Bridge-related primitives of the Rococo chain.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use bp_polkadot_core::*;

use bp_header_chain::ChainWithGrandpa;
use bp_runtime::{decl_bridge_finality_runtime_apis, Chain, ChainId};
use frame_support::{sp_runtime::StateVersion, weights::Weight};

/// Rococo Chain
pub struct Rococo;

impl Chain for Rococo {
	const ID: ChainId = *b"roco";

	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		max_extrinsic_weight()
	}
}

impl ChainWithGrandpa for Rococo {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = WITH_ROCOCO_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY: u32 =
		REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY;
	const MAX_MANDATORY_HEADER_SIZE: u32 = MAX_MANDATORY_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE: u32 = AVERAGE_HEADER_SIZE;
}

// The TransactionExtension used by Rococo.
pub use bp_polkadot_core::CommonTransactionExtension as TransactionExtension;

/// Name of the parachains pallet in the Rococo runtime.
pub const PARAS_PALLET_NAME: &str = "Paras";

/// Name of the With-Rococo GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_ROCOCO_GRANDPA_PALLET_NAME: &str = "BridgeRococoGrandpa";
/// Name of the With-Rococo parachains pallet instance that is deployed at bridged chains.
pub const WITH_ROCOCO_BRIDGE_PARACHAINS_PALLET_NAME: &str = "BridgeRococoParachains";

/// Maximal size of encoded `bp_parachains::ParaStoredHeaderData` structure among all Rococo
/// parachains.
///
/// It includes the block number and state root, so it shall be near 40 bytes, but let's have some
/// reserve.
pub const MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE: u32 = 128;

decl_bridge_finality_runtime_apis!(rococo, grandpa);
