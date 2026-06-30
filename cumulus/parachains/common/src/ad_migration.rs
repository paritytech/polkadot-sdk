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

//! Single-step multi-block migration that extends each existing
//! `pallet_session::SessionKeys` entry with a new `authority_discovery:
//! AuthorityDiscoveryId` field set to a per-validator placeholder derived from
//! the validator's aura raw bytes.
//!
//! Why a hashed placeholder rather than zero or aura-raw:
//! - The all-zero `sr25519::Public` is the Ristretto identity element, which schnorrkel accepts and
//!   against which any Schnorr signature is trivially forgeable (`s·B == R` collapses when `P ==
//!   0`). A zero placeholder is also shared by every validator across every adopting chain until
//!   they each rotate, letting any attacker squat the entire authority-discovery mesh.
//! - Reusing the aura raw bytes directly would tie the audi placeholder to the validator's aura
//!   secret, giving an unwanted cross-protocol key reuse for the duration of the rotation grace
//!   period.
//!
//! The hashed derivation is unique per validator and unforgeable.

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::DecodeAll;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Get,
	weights::WeightMeter,
};
#[cfg(feature = "try-runtime")]
use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{traits::OpaqueKeys, KeyTypeId, RuntimeAppPublic};

#[cfg(feature = "try-runtime")]
#[frame_support::storage_alias]
type OldNextKeys<R: pallet_session::Config> = StorageMap<
	pallet_session::Pallet<R>,
	Twox64Concat,
	<R as pallet_session::Config>::ValidatorId,
	AuraId,
	OptionQuery,
>;

const LOG_TARGET: &str = "runtime::ad_migration";

/// Unique pallet identifier for this authority-discovery key migration.
///
/// Used to build the [`MigrationId`] the framework records once the migration completes.
const MIGRATION_ID: &[u8; 16] = b"para_cmn_add_adi";

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

/// Per-validator placeholder authority-discovery key.
fn placeholder_audi(aura: &AuraId) -> AuthorityDiscoveryId {
	let aura_raw: [u8; 32] = sp_core::sr25519::Public::from(aura.clone()).0;
	let hash = sp_io::hashing::blake2_256(&aura_raw);
	sp_core::sr25519::Public::from_raw(hash).into()
}

/// `true` iff `R::Keys` exposes exactly `[aura, authority_discovery]`.
fn layout_matches<R: pallet_session::Config>() -> bool {
	<<R as pallet_session::Config>::Keys as OpaqueKeys>::key_ids() ==
		[<AuraId as RuntimeAppPublic>::ID, <AuthorityDiscoveryId as RuntimeAppPublic>::ID]
}

/// Extend `pallet_session` session-key records with an authority-discovery placeholder.
///
/// Implemented as a single-step [`SteppedMigration`]: it completes in exactly one
/// [`step`](SteppedMigration::step) and returns a `None` cursor, after which the
/// multi-block migration framework records it as done and never calls it again.
pub struct AppendAuthorityDiscoveryKeys<R>(PhantomData<R>);

impl<R> SteppedMigration for AppendAuthorityDiscoveryKeys<R>
where
	R: pallet_session::Config,
{
	// Single step, no progress state to carry between steps.
	type Cursor = ();
	type Identifier = MigrationId<16>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *MIGRATION_ID, version_from: 0, version_to: 1 }
	}

	fn max_steps() -> Option<u32> {
		Some(1)
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		if cursor.is_some() {
			return Ok(None);
		}

		// Refuse to touch storage unless `R::Keys` exposes exactly `[aura, authority_discovery]`.
		if !layout_matches::<R>() {
			log::error!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: R::Keys::key_ids() != [aura, authority_discovery]; \
				 aborting without touching storage",
			);
			return Err(SteppedMigrationError::Failed);
		}

		// Compute the worst-case weight before doing any work, then charge the meter. `decode_len`
		// reads only the length-prefix of `Validators`, not the whole vector.
		let db = <R as frame_system::Config>::DbWeight::get();
		let n = pallet_session::Validators::<R>::decode_len().unwrap_or_default() as u64;
		// Weight: per validator `upgrade_keys`:
		//   - 1 read  + 1 write : NextKeys translate
		//   - 1 write           : KeyOwner remove (old aura)
		//   - 2 writes          : KeyOwner insert (new aura + new audi)
		// Plus constant ops: QueuedKeys translate (1R + 1W) and the Validators length read (1R).
		// Total: (n × 1 + 2) reads, (n × 4 + 1) writes.
		let weight = db.reads_writes(
			n.saturating_mul(1).saturating_add(2),
			n.saturating_mul(4).saturating_add(1),
		);
		meter
			.try_consume(weight)
			.map_err(|_| SteppedMigrationError::InsufficientWeight { required: weight })?;

		pallet_session::Pallet::<R>::upgrade_keys::<OldSessionKeys, _>(|collator, old| {
			log::info!(
				target: LOG_TARGET,
				"Collator {:?}: authority-discovery placeholder installed; rotate to activate",
				collator,
			);
			let audi = placeholder_audi(&old.aura);
			let bytes = (old.aura, audi).encode();
			<R as pallet_session::Config>::Keys::decode(&mut &bytes[..])
				.expect("R::Keys::key_ids() verified above; qed")
		});

		log::info!(
			target: LOG_TARGET,
			"AppendAuthorityDiscoveryKeys: migrated ~{} collator(s) with hashed placeholder keys",
			n,
		);

		// Single step: signal completion.
		Ok(None)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		if !layout_matches::<R>() {
			return Err(sp_runtime::TryRuntimeError::Other(
				"pre_upgrade: R::Keys::key_ids() != [aura, authority_discovery]",
			));
		}

		// Snapshot all NextKeys entries under the OLD layout.
		let next_keys_state: Vec<(<R as pallet_session::Config>::ValidatorId, AuraId)> =
			OldNextKeys::<R>::iter().collect();

		log::info!(
			target: LOG_TARGET,
			"pre_upgrade: NextKeys entries={}",
			next_keys_state.len(),
		);

		Ok(next_keys_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let pre_next_keys: Vec<(<R as pallet_session::Config>::ValidatorId, AuraId)> =
			DecodeAll::decode_all(&mut &state[..]).map_err(|_| {
				sp_runtime::TryRuntimeError::Other("post_upgrade: invalid state encoding")
			})?;

		// Every ValidatorId from pre-upgrade must still have a NextKeys entry that
		// preserves the original aura key and carries the hashed `placeholder_audi`.
		for (id, aura) in &pre_next_keys {
			let keys = pallet_session::NextKeys::<R>::get(id).ok_or(
				sp_runtime::TryRuntimeError::Other(
					"post_upgrade: NextKeys entry missing after migration",
				),
			)?;
			let aura_bytes: &[u8] = aura.as_ref();
			if keys.get_raw(<AuraId as RuntimeAppPublic>::ID) != aura_bytes {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: aura key not preserved by migration",
				));
			}
			let expected_audi = placeholder_audi(aura);
			let expected_audi_bytes: &[u8] = expected_audi.as_ref();
			if keys.get_raw(<AuthorityDiscoveryId as RuntimeAppPublic>::ID) != expected_audi_bytes {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: authority-discovery does not match the expected placeholder",
				));
			}
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

	pub type AccountId = u64;

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

	type Block = frame_system::mocking::MockBlock<Test>;

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
			sp_io::storage::set(
				&pallet_session::NextKeys::<Test>::hashed_key_for(v),
				&aura.encode(),
			);
			pallet_session::KeyOwner::<Test>::insert(
				(<AuraId as RuntimeAppPublic>::ID, aura.encode()),
				v,
			);
		}
		sp_io::storage::set(&pallet_session::QueuedKeys::<Test>::hashed_key(), &queued.encode());
		pallet_session::Validators::<Test>::put(validators.to_vec());
	}

	/// Drive the single-step migration. Asserts it completes in one step (`Ok(None)`), and when
	/// `try-runtime` is enabled also exercises the `pre_upgrade` → `step` → `post_upgrade` hooks
	/// the multi-block migration framework runs in CI.
	fn run_migration() {
		#[cfg(feature = "try-runtime")]
		let state = AppendAuthorityDiscoveryKeys::<Test>::pre_upgrade().unwrap();

		let cursor =
			AppendAuthorityDiscoveryKeys::<Test>::step(None, &mut WeightMeter::new()).unwrap();
		assert_eq!(cursor, None, "single-step migration must complete in one step");

		#[cfg(feature = "try-runtime")]
		AppendAuthorityDiscoveryKeys::<Test>::post_upgrade(state).unwrap();
	}

	#[test]
	fn appends_placeholder_audi_keys() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2, 3];
			seed_old_layout(&validators);

			run_migration();

			for v in validators {
				let keys = pallet_session::NextKeys::<Test>::get(v).expect("entry must remain");
				let expected_audi = placeholder_audi(&aura_key(v));
				assert_eq!(keys.aura, aura_key(v));
				assert_eq!(keys.authority_discovery, expected_audi);
				assert_eq!(
					pallet_session::KeyOwner::<Test>::get((
						<AuraId as RuntimeAppPublic>::ID,
						aura_key(v).encode(),
					)),
					Some(v),
				);
				// Each validator has its own per-validator KeyOwner entry for the
				// aura-derived audi placeholder — no last-writer-wins collision.
				assert_eq!(
					pallet_session::KeyOwner::<Test>::get((
						<AuthorityDiscoveryId as RuntimeAppPublic>::ID,
						expected_audi.encode(),
					)),
					Some(v),
				);
			}

			let queued = pallet_session::QueuedKeys::<Test>::get();
			assert_eq!(queued.len(), validators.len());
			for (v, keys) in &queued {
				assert_eq!(keys.aura, aura_key(*v));
				assert_eq!(keys.authority_discovery, placeholder_audi(&aura_key(*v)));
			}
		});
	}

	#[test]
	fn completes_in_a_single_step() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2];
			seed_old_layout(&validators);

			// The first (and only) step signals completion by returning a `None` cursor; the
			// framework then records the migration as done and never calls it again.
			let cursor =
				AppendAuthorityDiscoveryKeys::<Test>::step(None, &mut WeightMeter::new()).unwrap();
			assert_eq!(cursor, None);
		});
	}

	#[test]
	fn works_with_empty_session_storage() {
		new_test_ext().execute_with(|| {
			run_migration();
			assert!(pallet_session::Validators::<Test>::get().is_empty());
		});
	}

	/// Second mock runtime with a `SessionKeys` layout the migration must reject:
	/// fields swapped to `{authority_discovery, aura}`. Exercises the `layout_matches`
	/// guard's abort-without-touching-storage path.
	mod wrong_layout {
		use super::{super::*, IdentityValidator, Offset, Period};
		use frame_support::derive_impl;
		use sp_runtime::BuildStorage;

		type AccountId = super::AccountId;
		type Block = frame_system::mocking::MockBlock<Test>;

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

		// KEY_TYPE_IDS is reversed compared to the outer TestSessionHandler; keep local.
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
				let mut meter = WeightMeter::new();
				let result = AppendAuthorityDiscoveryKeys::<Test>::step(None, &mut meter);
				assert_eq!(result, Err(SteppedMigrationError::Failed));
				// Aborted before charging the meter or writing anything.
				assert_eq!(meter.consumed(), frame_support::weights::Weight::zero());
				assert!(pallet_session::QueuedKeys::<Test>::get().is_empty());
			});
		}
	}
}
