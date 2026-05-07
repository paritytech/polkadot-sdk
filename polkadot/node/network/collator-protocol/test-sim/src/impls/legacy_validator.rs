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

//! Legacy validator-side adapter: drives the production `ProtocolSide::Validator` variant of
//! [`polkadot_collator_protocol::CollatorProtocolSubsystem`].

use crate::{
	harness::sim::SubsystemUnderTest,
	runtime::{LocalPoolSpawner, MockClock},
};
use futures::{future::BoxFuture, FutureExt};
use polkadot_collator_protocol::{CollatorProtocolSubsystem, ProtocolSide};
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage},
	overseer::Subsystem,
	SpawnGlue,
};
use polkadot_node_subsystem_test_helpers::TestSubsystemContext;
use sp_keystore::Keystore;
use std::{collections::HashSet, sync::Arc};

/// Adapter for the legacy `ProtocolSide::Validator` variant.
pub struct LegacyValidator;

impl SubsystemUnderTest for LegacyValidator {
	type Message = CollatorProtocolMessage;

	fn spawn(
		ctx: TestSubsystemContext<Self::Message, SpawnGlue<LocalPoolSpawner>>,
		clock: Arc<MockClock>,
	) -> BoxFuture<'static, ()> {
		let keystore: sp_keystore::KeystorePtr = Arc::new(sc_keystore::LocalKeystore::in_memory());
		// Insert a single Sr25519 key so the keystore is non-empty (the production code path
		// expects keys present for sign-on-second).
		Keystore::sr25519_generate_new(
			&*keystore,
			polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
			Some(&sp_keyring::Sr25519Keyring::Alice.to_seed()),
		)
		.expect("keystore accepts inserted key");

		let side = ProtocolSide::Validator {
			keystore,
			eviction_policy: Default::default(),
			metrics: Default::default(),
			invulnerables: HashSet::new(),
			collator_protocol_hold_off: None,
			clock,
		};
		let subsystem = CollatorProtocolSubsystem::new(side);
		let spawned = subsystem.start(ctx);
		spawned.future.map(|_| ()).boxed()
	}

	fn try_extract_inbound(msg: AllMessages) -> Result<Self::Message, AllMessages> {
		match msg {
			AllMessages::CollatorProtocol(inner) => Ok(inner),
			other => Err(other),
		}
	}
}
