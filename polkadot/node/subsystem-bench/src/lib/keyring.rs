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

use polkadot_primitives::ValidatorId;
use sc_keystore::LocalKeystore;
use sp_application_crypto::AppCrypto;
use sp_core::sr25519::Public;
use sp_keystore::Keystore;
use std::sync::Arc;

/// Set of test accounts generated and kept safe by a keystore.
#[derive(Clone)]
pub struct Keyring {
	keystore: Arc<LocalKeystore>,
}

impl Default for Keyring {
	fn default() -> Self {
		Self { keystore: Arc::new(LocalKeystore::in_memory()) }
	}
}

impl Keyring {
	pub fn sr25519_new(&self, seed: &str) -> Public {
		self.keystore
			.sr25519_generate_new(ValidatorId::ID, Some(seed))
			.expect("Insert key into keystore")
	}

	pub fn keystore(&self) -> Arc<dyn Keystore> {
		self.keystore.clone()
	}

	pub fn keystore_ref(&self) -> &LocalKeystore {
		self.keystore.as_ref()
	}
}
