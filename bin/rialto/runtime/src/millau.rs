// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Configuration parameters for the Millau Substrate chain.

use bp_rialto::Header;
use hex_literal::hex;
use pallet_substrate_bridge::{AuthoritySet, InitializationData};
use sp_core::crypto::Public;
use sp_finality_grandpa::AuthorityId;
use sp_std::vec;

/// Information about where the bridge palelt should start syncing from. This includes things like
/// the initial header and the initial authorities of the briged chain.
pub fn init_data() -> InitializationData<Header> {
	let authority_set = initial_authority_set();
	InitializationData {
		header: initial_header(),
		authority_list: authority_set.authorities,
		set_id: authority_set.set_id,
		scheduled_change: None,
		is_halted: false,
	}
}

/// The first header known to the pallet.
///
/// Note that this does not need to be the genesis header of the Millau
/// chain since the pallet may start at any arbitrary header.
// To get this we first need to call the `chain_getBlockHash` RPC method, and then
// we can use the result from that and call the `chain_getBlock` RPC method to get
// the rest of the info.
//
// In this case we've grabbed the genesis block of the Millau Substrate chain.
pub fn initial_header() -> Header {
	Header {
		parent_hash: Default::default(),
		number: Default::default(),
		state_root: hex!("0f2ca6dde08378ef81958bf087a3c40391079d0dbf434ea3fa0f73d54200839b").into(),
		extrinsics_root: hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314").into(),
		digest: Default::default(),
	}
}

/// The first set of Grandpa authorities known to the pallet.
///
/// Note that this doesn't have to be the "genesis" authority set, as the
/// pallet can be configured to start from any height.
pub fn initial_authority_set() -> AuthoritySet {
	let set_id = 0;

	// These authorities are: Alice, Bob, Charlie, Dave, and Eve.
	let authorities = vec![
		(
			AuthorityId::from_slice(&hex!(
				"88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee"
			)),
			1,
		),
		(
			AuthorityId::from_slice(&hex!(
				"d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae69"
			)),
			1,
		),
		(
			AuthorityId::from_slice(&hex!(
				"439660b36c6c03afafca027b910b4fecf99801834c62a5e6006f27d978de234f"
			)),
			1,
		),
		(
			AuthorityId::from_slice(&hex!(
				"5e639b43e0052c47447dac87d6fd2b6ec50bdd4d0f614e4299c665249bbd09d9"
			)),
			1,
		),
		(
			AuthorityId::from_slice(&hex!(
				"1dfe3e22cc0d45c70779c1095f7489a8ef3cf52d62fbd8c2fa38c9f1723502b5"
			)),
			1,
		),
	];

	AuthoritySet::new(authorities, set_id)
}
