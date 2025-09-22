// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime API definition for the broadcaster pallet.

use alloc::vec::Vec;
use polkadot_primitives::Id as ParaId;

sp_api::decl_runtime_apis! {
	/// The API for querying published data from parachains.
	#[api_version(1)]
	pub trait BroadcasterApi {
		/// Get published value for a specific parachain and key.
		/// Returns None if the parachain hasn't published data or the key doesn't exist.
		fn get_published_value(para_id: ParaId, key: Vec<u8>) -> Option<Vec<u8>>;

		/// Get the child trie root hash for a publisher.
		/// This can be used to prove the current state of published data.
		fn get_publisher_child_root(para_id: ParaId) -> Option<Vec<u8>>;

		/// Get all published data for a specific parachain.
		/// Returns empty vec if the parachain hasn't published any data.
		fn get_all_published_data(para_id: ParaId) -> Vec<(Vec<u8>, Vec<u8>)>;

		/// Get list of all parachains that have published data.
		/// Returns empty vec if no parachains have published data.
		fn get_all_publishers() -> Vec<ParaId>;
	}
}