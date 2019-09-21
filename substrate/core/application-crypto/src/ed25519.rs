// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Ed25519 crypto types.

use crate::{RuntimePublic, KeyTypeId};

pub use primitives::ed25519::*;

mod app {
	use primitives::testing::ED25519;
	crate::app_crypto!(super, ED25519);
}

pub use app::Public as AppPublic;
pub use app::Signature as AppSignature;
#[cfg(feature="std")]
pub use app::Pair as AppPair;

impl RuntimePublic for Public {
	type Signature = Signature;

	fn all(key_type: KeyTypeId) -> crate::Vec<Self> {
		runtime_io::ed25519_public_keys(key_type)
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<&str>) -> Self {
		runtime_io::ed25519_generate(key_type, seed)
	}

	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature> {
		runtime_io::ed25519_sign(key_type, self, msg.as_ref())
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		runtime_io::ed25519_verify(&signature, msg.as_ref(), self)
	}
}

#[cfg(test)]
mod tests {
	use sr_primitives::{generic::BlockId, traits::ProvideRuntimeApi};
	use primitives::{
				testing::{
						KeyStore,
						ED25519,
				},
				crypto::Pair,
				traits::BareCryptoStore as _,
		};
	use test_client::{
		TestClientBuilder, DefaultTestClientBuilderExt, TestClientBuilderExt,
		runtime::{TestAPI, app_crypto::ed25519::{AppPair, AppPublic}},
	};

	#[test]
	fn ed25519_works_in_runtime() {
		let keystore = KeyStore::new();
		let test_client = TestClientBuilder::new().set_keystore(keystore.clone()).build();
		let (signature, public) = test_client.runtime_api()
			.test_ed25519_crypto(&BlockId::Number(0))
			.expect("Tests `ed25519` crypto.");

		let key_pair = keystore.read().ed25519_key_pair(ED25519, &public.as_ref())
			.expect("There should be at a `ed25519` key in the keystore for the given public key.");

		assert!(AppPair::verify(&signature, "ed25519", &AppPublic::from(key_pair.public())));
	}
}
