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
//! AuthorityDiscoveryId` field. By default the field is set to a per-validator placeholder derived
//! from the validator's aura raw bytes. But each runtime has the option to customize this behavior.
//! See [`KeysTranslatorT::gen_authority_discovery`] for more details.

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
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_core::ByteArray;
pub use sp_runtime;
use sp_runtime::{
	traits::{Member, OpaqueKeys},
	RuntimeAppPublic,
};

const LOG_TARGET: &str = "runtime::ad_migration";

/// Unique pallet identifier for this authority-discovery key migration.
///
/// Used to build the [`MigrationId`] the framework records once the migration completes.
const MIGRATION_ID: &[u8; 16] = b"para_cmn_add_adi";

pub trait KeysTranslatorT {
	type AuraId: Encode + Decode + Clone + AsRef<[u8]> + ByteArray + RuntimeAppPublic;
	type OldSessionKeys: Decode + Member + OpaqueKeys;
	type NewSessionKeys: 'static + codec::FullCodec + OpaqueKeys;

	fn aura(old: &Self::OldSessionKeys) -> &Self::AuraId;

	/// Generate an `authority_discovery` key.
	///
	/// The default implementation generates a per-validator placeholder derived from the
	/// validator's aura raw bytes.
	///
	/// Why a hashed placeholder rather than zero or aura-raw:
	/// - for example for `sr25519::Public` the all-zero key is the Ristretto identity element,
	///   which schnorrkel accepts and against which any Schnorr signature is trivially forgeable
	///   (`s·B == R` collapses when `P == 0`). A zero placeholder is also shared by every validator
	///   across every adopting chain until they each rotate, letting any attacker squat the entire
	///   authority-discovery mesh.
	/// - Reusing the aura raw bytes directly would tie the authority discovery placeholder to the
	///   validator's aura secret, giving an unwanted cross-protocol key reuse for the duration of
	///   the rotation grace period.
	///
	/// The hashed derivation is unique per validator and unforgeable.
	fn gen_authority_discovery(aura: &Self::AuraId) -> AuthorityDiscoveryId {
		let hash = sp_io::hashing::blake2_256(aura.as_slice());
		sp_core::sr25519::Public::from_raw(hash).into()
	}

	fn do_translate(
		old: Self::OldSessionKeys,
		authority_discovery: AuthorityDiscoveryId,
	) -> Self::NewSessionKeys;

	fn translate(old: Self::OldSessionKeys) -> Self::NewSessionKeys {
		let aura = Self::aura(&old);
		let authority_discovery = Self::gen_authority_discovery(&aura);
		Self::do_translate(old, authority_discovery)
	}
}

/// Generates a default `KeysTranslatorT` implementation for the case where the old session keys
/// were `{aura}` and we want to append the `authority_discovery` key.
#[macro_export]
macro_rules! impl_keys_translator {
	($session_keys: ident, $aura_id: ident, $authority_discovery_id: ident) => {
		$crate::ad_migration::sp_runtime::impl_opaque_keys! {
			pub struct OldSessionKeys {
				pub aura: $aura_id,
			}
		}

		pub struct KeysTranslator;

		impl $crate::ad_migration::KeysTranslatorT for KeysTranslator {
			type AuraId = $aura_id;
			type OldSessionKeys = OldSessionKeys;
			type NewSessionKeys = $session_keys;

			fn aura(old: &Self::OldSessionKeys) -> &Self::AuraId {
				&old.aura
			}

			fn do_translate(
				old: Self::OldSessionKeys,
				authority_discovery: $authority_discovery_id,
			) -> Self::NewSessionKeys {
				Self::NewSessionKeys { aura: old.aura, authority_discovery }
			}
		}
	};
}

/// Extend `pallet_session` session-key records with an authority-discovery placeholder.
pub struct AppendAuthorityDiscoveryKeys<R, Translator>(PhantomData<(R, Translator)>);

impl<R, Translator> SteppedMigration for AppendAuthorityDiscoveryKeys<R, Translator>
where
	R: pallet_session::Config<Keys = Translator::NewSessionKeys>,
	Translator: KeysTranslatorT,
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
		let weight = db.reads_writes(n.saturating_add(2), n.saturating_mul(4).saturating_add(1));
		meter
			.try_consume(weight)
			.map_err(|_| SteppedMigrationError::InsufficientWeight { required: weight })?;

		pallet_session::Pallet::<R>::upgrade_keys::<Translator::OldSessionKeys, _>(
			|collator, old| {
				log::info!(
					target: LOG_TARGET,
					"Collator {:?}: authority-discovery placeholder installed; rotate to activate",
					collator,
				);
				Translator::translate(old)
			},
		);

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
		// Snapshot the pre-migration NextKeys, decoding the existing values with the same
		// `OldSessionKeys` type that `upgrade_keys` consumes.
		let next_keys_state: Vec<(<R as pallet_session::Config>::ValidatorId, Translator::AuraId)> =
			pallet_session::NextKeys::<R>::iter_keys()
				.filter_map(|id| {
					let key = pallet_session::NextKeys::<R>::hashed_key_for(&id);
					frame_support::storage::unhashed::get::<Translator::OldSessionKeys>(&key)
						.map(|old| (id, Translator::aura(&old).clone()))
				})
				.collect();

		log::info!(
			target: LOG_TARGET,
			"pre_upgrade: NextKeys entries={}",
			next_keys_state.len(),
		);

		Ok(next_keys_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let pre_next_keys: Vec<(<R as pallet_session::Config>::ValidatorId, Translator::AuraId)> =
			DecodeAll::decode_all(&mut &state[..]).map_err(|_| {
				sp_runtime::TryRuntimeError::Other("post_upgrade: invalid state encoding")
			})?;

		// Every ValidatorId from pre-upgrade must still have a NextKeys entry that
		// preserves the original aura key and carries the hashed `placeholder_audi`.
		for (id, aura) in &pre_next_keys {
			let next_keys = pallet_session::NextKeys::<R>::get(id).ok_or(
				sp_runtime::TryRuntimeError::Other(
					"post_upgrade: NextKeys entry missing after migration",
				),
			)?;
			if next_keys.get_raw(Translator::AuraId::ID) != aura.as_slice() {
				return Err(sp_runtime::TryRuntimeError::Other(
					"post_upgrade: aura key not preserved by migration",
				));
			}
			let expected_authority_discovery = Translator::gen_authority_discovery(&aura);
			if next_keys.get_raw(AuthorityDiscoveryId::ID) !=
				expected_authority_discovery.as_slice()
			{
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
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_runtime::{BuildStorage, KeyTypeId};

	fn new_test_ext<Runtime: frame_system::Config>() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap()
			.into()
	}

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

	impl_keys_translator!(MockSessionKeys, AuraId, AuthorityDiscoveryId);

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
		const KEY_TYPE_IDS: &'static [KeyTypeId] = &[AuraId::ID, AuthorityDiscoveryId::ID];
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
	/// `try-runtime` is enabled also exercises the `pre_upgrade` → `step` → `post_upgrade`
	/// hooks the multi-block migration framework runs in CI.
	fn run_migration() {
		#[cfg(feature = "try-runtime")]
		let state = AppendAuthorityDiscoveryKeys::<Test, KeysTranslator>::pre_upgrade().unwrap();

		let cursor = AppendAuthorityDiscoveryKeys::<Test, KeysTranslator>::step(
			None,
			&mut WeightMeter::new(),
		)
		.unwrap();
		assert_eq!(cursor, None, "single-step migration must complete in one step");

		#[cfg(feature = "try-runtime")]
		AppendAuthorityDiscoveryKeys::<Test, KeysTranslator>::post_upgrade(state).unwrap();
	}

	#[test]
	fn appends_placeholder_audi_keys() {
		new_test_ext::<Test>().execute_with(|| {
			let validators = [1, 2, 3];
			seed_old_layout(&validators);

			run_migration();

			for v in validators {
				let keys = pallet_session::NextKeys::<Test>::get(v).expect("entry must remain");
				let expected_authority_discovery =
					KeysTranslator::gen_authority_discovery(&aura_key(v));
				assert_eq!(keys.aura, aura_key(v));
				assert_eq!(keys.authority_discovery, expected_authority_discovery);
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
						expected_authority_discovery.encode(),
					)),
					Some(v),
				);
			}

			let queued = pallet_session::QueuedKeys::<Test>::get();
			assert_eq!(queued.len(), validators.len());
			for (v, keys) in &queued {
				assert_eq!(keys.aura, aura_key(*v));
				assert_eq!(
					keys.authority_discovery,
					KeysTranslator::gen_authority_discovery(&aura_key(*v))
				);
			}
		});
	}

	#[test]
	fn completes_in_a_single_step() {
		new_test_ext::<Test>().execute_with(|| {
			let validators = [1, 2];
			seed_old_layout(&validators);

			// The first (and only) step signals completion by returning a `None` cursor; the
			// framework then records the migration as done and never calls it again.
			let cursor = AppendAuthorityDiscoveryKeys::<Test, KeysTranslator>::step(
				None,
				&mut WeightMeter::new(),
			)
			.unwrap();
			assert_eq!(cursor, None);
		});
	}

	#[test]
	fn works_with_empty_session_storage() {
		new_test_ext::<Test>().execute_with(|| {
			run_migration();
			assert!(pallet_session::Validators::<Test>::get().is_empty());
		});
	}
}
