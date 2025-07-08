// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Defines structures related to calls of the `pallet-bridge-parachains` pallet.

use crate::{ParaHash, ParaId, RelayBlockHash, RelayBlockNumber};

use bp_polkadot_core::parachains::ParaHeadsProof;
use bp_runtime::HeaderId;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

/// A minimized version of `pallet-bridge-parachains::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeParachainCall {
	/// `pallet-bridge-parachains::Call::submit_parachain_heads`
	#[codec(index = 0)]
	submit_parachain_heads {
		/// Relay chain block, for which we have submitted the `parachain_heads_proof`.
		at_relay_block: (RelayBlockNumber, RelayBlockHash),
		/// Parachain identifiers and their head hashes.
		parachains: Vec<(ParaId, ParaHash)>,
		/// Parachain heads proof.
		parachain_heads_proof: ParaHeadsProof,
	},
}

/// Info about a `SubmitParachainHeads` call which tries to update a single parachain.
///
/// The pallet supports updating multiple parachain heads at once,
#[derive(PartialEq, RuntimeDebug)]
pub struct SubmitParachainHeadsInfo {
	/// Number and hash of the finalized relay block that has been used to prove parachain
	/// finality.
	pub at_relay_block: HeaderId<RelayBlockHash, RelayBlockNumber>,
	/// Parachain identifier.
	pub para_id: ParaId,
	/// Hash of the bundled parachain head.
	pub para_head_hash: ParaHash,
	/// If `true`, then the call must be free (assuming that everything else is valid) to
	/// be treated as valid.
	pub is_free_execution_expected: bool,
}
