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

//! Utilities for working with test accounts.

use bp_header_chain::{justification::JustificationVerificationContext, AuthoritySet};
use codec::Encode;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use finality_grandpa::voter_set::VoterSet;
use sp_consensus_grandpa::{AuthorityId, AuthorityList, AuthorityWeight, SetId};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Set of test accounts with friendly names: Alice.
pub const ALICE: Account = Account(0);
/// Set of test accounts with friendly names: Bob.
pub const BOB: Account = Account(1);
/// Set of test accounts with friendly names: Charlie.
pub const CHARLIE: Account = Account(2);
/// Set of test accounts with friendly names: Dave.
pub const DAVE: Account = Account(3);
/// Set of test accounts with friendly names: Eve.
pub const EVE: Account = Account(4);
/// Set of test accounts with friendly names: Ferdie.
pub const FERDIE: Account = Account(5);

/// A test account which can be used to sign messages.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Account(pub u16);

impl Account {
	/// Returns public key of this account.
	pub fn public(&self) -> VerifyingKey {
		self.pair().verifying_key()
	}

	/// Returns key pair, used to sign data on behalf of this account.
	pub fn pair(&self) -> SigningKey {
		let data = self.0.encode();
		let mut bytes = [0_u8; 32];
		bytes[0..data.len()].copy_from_slice(&data);
		SigningKey::from_bytes(&bytes)
	}

	/// Generate a signature of given message.
	pub fn sign(&self, msg: &[u8]) -> Signature {
		use ed25519_dalek::Signer;
		self.pair().sign(msg)
	}
}

impl From<Account> for AuthorityId {
	fn from(p: Account) -> Self {
		sp_application_crypto::UncheckedFrom::unchecked_from(p.public().to_bytes())
	}
}

/// Get a valid set of voters for a Grandpa round.
pub fn voter_set() -> VoterSet<AuthorityId> {
	VoterSet::new(authority_list()).unwrap()
}

/// Get a valid justification verification context for a GRANDPA round.
pub fn verification_context(set_id: SetId) -> JustificationVerificationContext {
	AuthoritySet { authorities: authority_list(), set_id }.try_into().unwrap()
}

/// Convenience function to get a list of Grandpa authorities.
pub fn authority_list() -> AuthorityList {
	test_keyring().iter().map(|(id, w)| (AuthorityId::from(*id), *w)).collect()
}

/// Get the corresponding identities from the keyring for the "standard" authority set.
pub fn test_keyring() -> Vec<(Account, AuthorityWeight)> {
	vec![(ALICE, 1), (BOB, 1), (CHARLIE, 1)]
}

/// Get a list of "unique" accounts.
pub fn accounts(len: u16) -> Vec<Account> {
	(0..len).map(Account).collect()
}
