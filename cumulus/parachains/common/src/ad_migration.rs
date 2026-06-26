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

//! One-shot migration that extends each existing `pallet_session::SessionKeys`
//! entry with a new `authority_discovery: AuthorityDiscoveryId` field set to
//! a 32-byte zero placeholder.

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::DecodeAll;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_io::storage;
use sp_runtime::{traits::OpaqueKeys, KeyTypeId, RuntimeAppPublic};

const LOG_TARGET: &str = "runtime::ad_migration";

/// If set it means this migration was already executed.
const MIGRATION_DONE_KEY: &[u8] = b":parachains_common:ad_migration:done";

/// `SessionKeys` layout before `pallet-authority-discovery` was added.
#[derive(Clone, Eq, PartialEq, Debug, Decode)]
struct OldSessionKeys {
	pub aura: AuraId,
}

impl OpaqueKeys for OldSessionKeys {
	type KeyTypeIdProviders = (AuraId,);

	fn key_ids() -> &'static [KeyTypeId] {
		&[<AuraId as RuntimeAppPublic>::ID]
	}

	fn get_raw(&self, i: KeyTypeId) -> &[u8] {
		if i == <AuraId as RuntimeAppPublic>::ID {
			self.aura.as_ref()
		} else {
			&[]
		}
	}

	fn ownership_proof_is_valid(&self, _owner: &[u8], _proof: &[u8]) -> bool {
		// `pallet_session::Pallet::upgrade_keys` never calls this on the `Old` type.
		false
	}
}

fn zero_audi() -> AuthorityDiscoveryId {
	AuthorityDiscoveryId::from(sp_core::sr25519::Public::from_raw([0u8; 32]))
}

fn migration_done() -> bool {
	storage::get(MIGRATION_DONE_KEY).is_some()
}

/// `true` iff `R::Keys` exposes exactly `[aura, authority_discovery]`.
fn layout_matches<R: pallet_session::Config>() -> bool {
	<<R as pallet_session::Config>::Keys as OpaqueKeys>::key_ids() ==
		[<AuraId as RuntimeAppPublic>::ID, <AuthorityDiscoveryId as RuntimeAppPublic>::ID]
}

#[cfg(feature = "try-runtime")]
fn next_keys_prefix<R: pallet_session::Config>() -> [u8; 32] {
	<pallet_session::NextKeys<R> as frame_support::storage::StoragePrefixedMap<_>>::final_prefix()
}

#[cfg(feature = "try-runtime")]
fn queued_keys_key<R: pallet_session::Config>() -> [u8; 32] {
	pallet_session::QueuedKeys::<R>::hashed_key()
}

/// Extend `pallet_session` session-key records with an authority-discovery placeholder.
pub struct AppendAuthorityDiscoveryKeys<R>(PhantomData<R>);

impl<R> OnRuntimeUpgrade for AppendAuthorityDiscoveryKeys<R>
where
	R: pallet_session::Config,
{
	fn on_runtime_upgrade() -> Weight {
		let db = <R as frame_system::Config>::DbWeight::get();

		// Skip if migration is done already.
		if migration_done() {
			log::info!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: already executed, skipping",
			);
			return db.reads(1);
		}

		// Refuse to touch storage unless R::Keys exposes exactly [aura, authority_discovery].
		if !layout_matches::<R>() {
			log::error!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: R::Keys::key_ids() != [aura, authority_discovery]; \
				 aborting without touching storage",
			);
			return db.reads(1);
		}

		let zeros = zero_audi();

		pallet_session::Pallet::<R>::upgrade_keys::<OldSessionKeys, _>(|collator, old| {
			log::info!(
				target: LOG_TARGET,
				"Collator {:?}: authority-discovery key set to zero",
				collator,
			);
			let bytes = (old.aura, zeros.clone()).encode();
			<R as pallet_session::Config>::Keys::decode(&mut &bytes[..])
				.expect("R::Keys::key_ids() verified above; qed")
		});

		storage::set(MIGRATION_DONE_KEY, &[1u8]);

		let n = pallet_session::Validators::<R>::get().len() as u64;
		log::info!(
			target: LOG_TARGET,
			"AppendAuthorityDiscoveryKeys: migrated ~{} collator(s) with zero placeholder keys",
			n,
		);

		// Weight: `upgrade_keys` reads/writes per-collator across NextKeys + KeyOwner
		// (old & new), plus one read/write for QueuedKeys, plus flag + Validators read.
		// `n` uses `Validators::get().len()` as a proxy to avoid a full `NextKeys` scan.
		// A chain with a pending validator rotation can have one queued set's worth of
		// extra `NextKeys` entries; the `+3 reads / +1 write` constant ops absorb that
		// discrepancy on every realistic system-chain validator set.
		db.reads_writes(
			n.saturating_mul(4).saturating_add(3),
			n.saturating_mul(4).saturating_add(1),
		)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		if migration_done() {
			log::info!(
				target: LOG_TARGET,
				"pre_upgrade: migration already executed on this chain; treating as a no-op",
			);
			return Ok(Vec::new());
		}

		if !layout_matches::<R>() {
			return Err(sp_runtime::TryRuntimeError::Other(
				"pre_upgrade: R::Keys::key_ids() != [aura, authority_discovery]",
			));
		}

		// Snapshot every NextKeys entry under the OLD (32-byte AuraId) layout. `DecodeAll`
		// rejects trailing bytes so we also fail if storage is already in the new layout.
		let prefix = next_keys_prefix::<R>();
		let mut next_keys_state: Vec<(Vec<u8>, AuraId)> = Vec::new();
		let mut cursor = prefix.to_vec();
		while let Some(next_key) = storage::next_key(&cursor) {
			if !next_key.starts_with(&prefix) {
				break;
			}
			cursor = next_key.clone();
			if let Some(raw) = storage::get(&next_key) {
				let aura = AuraId::decode_all(&mut &raw[..]).map_err(|_| {
					sp_runtime::TryRuntimeError::Other(
						"pre_upgrade: NextKeys entry not decodable as AuraId",
					)
				})?;
				next_keys_state.push((next_key, aura));
			}
		}

		let queued_key = queued_keys_key::<R>();
		let queued_present = storage::get(&queued_key).is_some();

		log::info!(
			target: LOG_TARGET,
			"pre_upgrade: NextKeys entries={}, QueuedKeys present={}",
			next_keys_state.len(),
			queued_present,
		);

		Ok((next_keys_state, queued_present).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		if state.is_empty() {
			if !migration_done() {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: sentinel missing after a detected re-run",
				));
			}
			return Ok(());
		}

		let (pre_next_keys, queued_present): (Vec<(Vec<u8>, AuraId)>, bool) =
			DecodeAll::decode_all(&mut &state[..]).map_err(|_| {
				sp_runtime::TryRuntimeError::Other(
					"post_upgrade: invalid pre-upgrade state encoding",
				)
			})?;

		if !migration_done() {
			return Err(sp_runtime::TryRuntimeError::Other(
				"post_upgrade: migration was not executed",
			));
		}

		// (b) Every NextKeys raw key from pre-upgrade must now decode as R::Keys, preserve
		// the original aura field, and carry a zero authority-discovery placeholder.
		let zero = zero_audi();
		let zero_bytes: &[u8] = zero.as_ref();
		for (raw_key, aura) in &pre_next_keys {
			let raw = storage::get(raw_key).ok_or(sp_runtime::TryRuntimeError::Other(
				"post_upgrade: NextKeys entry missing after migration",
			))?;
			let keys =
				<<R as pallet_session::Config>::Keys as DecodeAll>::decode_all(&mut &raw[..])
					.map_err(|_| {
						sp_runtime::TryRuntimeError::Other(
							"post_upgrade: NextKeys entry does not decode under new SessionKeys layout",
						)
					})?;
			let aura_bytes: &[u8] = aura.as_ref();
			if keys.get_raw(<AuraId as RuntimeAppPublic>::ID) != aura_bytes {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: aura key not preserved by migration",
				));
			}
			if keys.get_raw(<AuthorityDiscoveryId as RuntimeAppPublic>::ID) != zero_bytes {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: authority-discovery field is not the zero placeholder",
				));
			}
		}

		// (c) QueuedKeys must decode as Vec<(ValidatorId, R::Keys)> iff it was present before.
		let queued_key = queued_keys_key::<R>();
		match (queued_present, storage::get(&queued_key)) {
			(true, Some(raw)) => {
				type Queued<R> = Vec<(
					<R as pallet_session::Config>::ValidatorId,
					<R as pallet_session::Config>::Keys,
				)>;
				<Queued<R> as DecodeAll>::decode_all(&mut &raw[..]).map_err(|_| {
					sp_runtime::TryRuntimeError::Other(
						"post_upgrade: QueuedKeys does not decode under new layout",
					)
				})?;
			},
			(false, None) => {},
			(true, None) => {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: QueuedKeys missing after migration",
				))
			},
			(false, Some(_)) => {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: QueuedKeys present but was missing pre-upgrade",
				))
			},
		}

		log::info!(target: LOG_TARGET, "post_upgrade: OK");
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{derive_impl, parameter_types};
	use sp_runtime::BuildStorage;

	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = u64;

	sp_runtime::impl_opaque_keys! {
		pub struct MockSessionKeys {
			pub aura: AuraId,
			pub authority_discovery: AuthorityDiscoveryId,
		}
	}

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Balances: pallet_balances,
			Session: pallet_session,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountData = pallet_balances::AccountData<u64>;
		type DbWeight = frame_support::weights::constants::RocksDbWeight;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Test {
		type AccountStore = System;
	}

	parameter_types! {
		pub const Period: u64 = 10;
		pub const Offset: u64 = 0;
	}

	pub struct IdentityValidator;
	impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for IdentityValidator {
		fn convert(account: AccountId) -> Option<AccountId> {
			Some(account)
		}
	}

	pub struct TestSessionHandler;
	impl pallet_session::SessionHandler<AccountId> for TestSessionHandler {
		const KEY_TYPE_IDS: &'static [KeyTypeId] =
			&[<AuraId as RuntimeAppPublic>::ID, <AuthorityDiscoveryId as RuntimeAppPublic>::ID];
		fn on_genesis_session<K: OpaqueKeys>(_: &[(AccountId, K)]) {}
		fn on_new_session<K: OpaqueKeys>(_: bool, _: &[(AccountId, K)], _: &[(AccountId, K)]) {}
		fn on_disabled(_: u32) {}
	}

	impl pallet_session::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type ValidatorId = AccountId;
		type ValidatorIdOf = IdentityValidator;
		type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
		type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
		type SessionManager = ();
		type SessionHandler = TestSessionHandler;
		type Keys = MockSessionKeys;
		type DisablingStrategy = ();
		type WeightInfo = ();
		type Currency = Balances;
		type KeyDeposit = ();
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
	}

	fn aura_key(v: AccountId) -> AuraId {
		AuraId::from(sp_core::sr25519::Public::from_raw([v as u8; 32]))
	}

	/// Seed `NextKeys`/`QueuedKeys`/`KeyOwner`/`Validators` in the OLD (aura-only) layout,
	/// bypassing the typed API which only knows the new layout.
	fn seed_old_layout(validators: &[AccountId]) {
		let queued: Vec<(AccountId, AuraId)> =
			validators.iter().map(|v| (*v, aura_key(*v))).collect();
		for (v, aura) in &queued {
			storage::set(&pallet_session::NextKeys::<Test>::hashed_key_for(v), &aura.encode());
			pallet_session::KeyOwner::<Test>::insert(
				(<AuraId as RuntimeAppPublic>::ID, aura.encode()),
				v,
			);
		}
		storage::set(&pallet_session::QueuedKeys::<Test>::hashed_key(), &queued.encode());
		pallet_session::Validators::<Test>::put(validators.to_vec());
	}

	/// Run the migration through the try-runtime hooks when available, mirroring what
	/// `try-runtime on-runtime-upgrade` does in CI.
	fn run_migration() {
		#[cfg(feature = "try-runtime")]
		AppendAuthorityDiscoveryKeys::<Test>::try_on_runtime_upgrade(true).unwrap();
		#[cfg(not(feature = "try-runtime"))]
		AppendAuthorityDiscoveryKeys::<Test>::on_runtime_upgrade();
	}

	#[test]
	fn appends_placeholder_audi_keys() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2, 3];
			seed_old_layout(&validators);

			run_migration();

			for v in validators {
				let keys = pallet_session::NextKeys::<Test>::get(v).expect("entry must remain");
				assert_eq!(keys.aura, aura_key(v));
				assert_eq!(keys.authority_discovery, zero_audi());
				assert_eq!(
					pallet_session::KeyOwner::<Test>::get((
						<AuraId as RuntimeAppPublic>::ID,
						aura_key(v).encode(),
					)),
					Some(v),
				);
			}

			// The shared zero placeholder maps to a single owner (last writer wins).
			let zero_owner = pallet_session::KeyOwner::<Test>::get((
				<AuthorityDiscoveryId as RuntimeAppPublic>::ID,
				zero_audi().encode(),
			));
			assert!(zero_owner.is_some_and(|v| validators.contains(&v)));

			let queued = pallet_session::QueuedKeys::<Test>::get();
			assert_eq!(queued.len(), validators.len());
			for (v, keys) in &queued {
				assert_eq!(keys.aura, aura_key(*v));
				assert_eq!(keys.authority_discovery, zero_audi());
			}

			assert!(migration_done());
		});
	}

	#[test]
	fn second_run_is_a_noop() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2];
			seed_old_layout(&validators);
			run_migration();

			let before: Vec<_> =
				validators.iter().map(|v| pallet_session::NextKeys::<Test>::get(v)).collect();

			let weight = AppendAuthorityDiscoveryKeys::<Test>::on_runtime_upgrade();
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads(1));

			let after: Vec<_> =
				validators.iter().map(|v| pallet_session::NextKeys::<Test>::get(v)).collect();
			assert_eq!(before, after);

			// try-runtime re-runs.
			#[cfg(feature = "try-runtime")]
			{
				let state = AppendAuthorityDiscoveryKeys::<Test>::pre_upgrade().unwrap();
				assert!(state.is_empty());
				AppendAuthorityDiscoveryKeys::<Test>::post_upgrade(state).unwrap();
			}
		});
	}

	#[test]
	fn works_with_empty_session_storage() {
		new_test_ext().execute_with(|| {
			run_migration();
			assert!(migration_done());
		});
	}

	/// Second mock runtime with a `SessionKeys` layout the migration must reject:
	/// fields swapped to `{authority_discovery, aura}`. Exercises the `layout_matches`
	/// guard's abort-without-touching-storage path.
	mod wrong_layout {
		use super::super::*;
		use frame_support::{derive_impl, parameter_types};
		use sp_runtime::BuildStorage;

		type Block = frame_system::mocking::MockBlock<Test>;
		type AccountId = u64;

		sp_runtime::impl_opaque_keys! {
			pub struct WrongSessionKeys {
				pub authority_discovery: AuthorityDiscoveryId,
				pub aura: AuraId,
			}
		}

		frame_support::construct_runtime!(
			pub enum Test {
				System: frame_system,
				Balances: pallet_balances,
				Session: pallet_session,
			}
		);

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
		impl frame_system::Config for Test {
			type Block = Block;
			type AccountData = pallet_balances::AccountData<u64>;
			type DbWeight = frame_support::weights::constants::RocksDbWeight;
		}

		#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
		impl pallet_balances::Config for Test {
			type AccountStore = System;
		}

		parameter_types! {
			pub const Period: u64 = 10;
			pub const Offset: u64 = 0;
		}

		pub struct IdentityValidator;
		impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for IdentityValidator {
			fn convert(account: AccountId) -> Option<AccountId> {
				Some(account)
			}
		}

		pub struct TestSessionHandler;
		impl pallet_session::SessionHandler<AccountId> for TestSessionHandler {
			const KEY_TYPE_IDS: &'static [KeyTypeId] =
				&[<AuthorityDiscoveryId as RuntimeAppPublic>::ID, <AuraId as RuntimeAppPublic>::ID];
			fn on_genesis_session<K: OpaqueKeys>(_: &[(AccountId, K)]) {}
			fn on_new_session<K: OpaqueKeys>(_: bool, _: &[(AccountId, K)], _: &[(AccountId, K)]) {}
			fn on_disabled(_: u32) {}
		}

		impl pallet_session::Config for Test {
			type RuntimeEvent = RuntimeEvent;
			type ValidatorId = AccountId;
			type ValidatorIdOf = IdentityValidator;
			type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
			type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
			type SessionManager = ();
			type SessionHandler = TestSessionHandler;
			type Keys = WrongSessionKeys;
			type DisablingStrategy = ();
			type WeightInfo = ();
			type Currency = Balances;
			type KeyDeposit = ();
		}

		fn new_test_ext() -> sp_io::TestExternalities {
			frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
		}

		#[test]
		fn aborts_without_touching_storage() {
			new_test_ext().execute_with(|| {
				let weight = AppendAuthorityDiscoveryKeys::<Test>::on_runtime_upgrade();
				assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads(1));
				assert!(!migration_done());
			});
		}
	}
}
