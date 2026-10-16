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

//! Runtime migration that extends each existing `pallet_session::SessionKeys` entry with a new
//! `authority_discovery: AuthorityDiscoveryId` field.
//! By default the field is set to a per-validator placeholder derived from the validator's
//! aura raw bytes. But each runtime has the option to customize this behavior.
//! See [`KeysTranslatorT::gen_authority_discovery`] for more details.

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::DecodeAll;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	migrations::MigrationId,
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_core::ByteArray;
pub use sp_runtime;
use sp_runtime::{
	traits::{Member, OpaqueKeys},
	RuntimeAppPublic,
};

const LOG_TARGET: &str = "runtime::ad_migration";

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
	/// - for `sr25519::Public` the all-zero key is the Ristretto identity element, which schnorrkel
	///   accepts and against which any Schnorr signature is trivially forgeable (`s·B == R`
	///   collapses when `P == 0`). A zero placeholder is also shared by every validator across
	///   every adopting chain until they each rotate, letting any attacker squat the entire
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

impl<R, Translator> AppendAuthorityDiscoveryKeys<R, Translator>
where
	R: pallet_migrations::Config,
{
	/// Identifier recorded in [`pallet_migrations::Historic`] once this migration has been
	/// executed.
	///
	/// `pallet_migrations` only tracks multi-block migrations, but we reuse its `Historic` set so
	/// that all executed migrations are recorded in a single place.
	const MIGRATION_ID: MigrationId<16> =
		MigrationId { pallet_id: *b"para_cmn_add_adi", version_from: 0, version_to: 1 };

	fn bounded_id() -> Option<pallet_migrations::IdentifierOf<R>> {
		Self::MIGRATION_ID.encode().try_into().ok()
	}

	#[cfg(test)]
	fn was_executed() -> bool {
		let Some(id) = Self::bounded_id() else {
			return false;
		};

		pallet_migrations::Historic::<R>::contains_key(id)
	}

	fn try_execute(execute: impl Fn(&mut Weight)) -> Weight {
		let mut weight = Weight::zero();
		let db = <R as frame_system::Config>::DbWeight::get();

		let Some(id) = Self::bounded_id() else {
			log::error!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: Invalid ID. Skipping",
			);

			return weight;
		};

		weight = weight.saturating_add(db.reads(1));
		let was_executed = pallet_migrations::Historic::<R>::contains_key(&id);
		if was_executed {
			log::error!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: Migration was already executed. Skipping",
			);

			return weight;
		}

		execute(&mut weight);

		weight = weight.saturating_add(db.writes(1));
		pallet_migrations::Historic::<R>::insert(&id, ());

		weight
	}
}

impl<R, Translator> OnRuntimeUpgrade for AppendAuthorityDiscoveryKeys<R, Translator>
where
	R: pallet_session::Config<Keys = Translator::NewSessionKeys> + pallet_migrations::Config,
	Translator: KeysTranslatorT,
{
	fn on_runtime_upgrade() -> Weight {
		let weight = Self::try_execute(|weight| {
			let db = <R as frame_system::Config>::DbWeight::get();

			let num_keys = pallet_session::Pallet::<R>::upgrade_keys::<Translator::OldSessionKeys, _>(
				|collator, old| {
					log::info!(
						target: LOG_TARGET,
						"Collator {:?}: authority-discovery placeholder installed; rotate to activate",
						collator,
					);
					Translator::translate(old)
				},
			);

			// Per `NextKeys` entry translated by `upgrade_keys`:
			//   - 1 read  + 1 write : NextKeys translate
			//   - 1 write           : KeyOwner remove old aura + insert it back (same key)
			//   - 1 write           : KeyOwner insert authority_discovery
			let per_key_weight = db.reads_writes(1, 3);
			weight.saturating_accrue(per_key_weight.saturating_mul(num_keys));
			// QueuedKeys translate: 1 read + 1 write
			weight.saturating_accrue(db.reads_writes(1, 1));

			log::info!(
				target: LOG_TARGET,
				"AppendAuthorityDiscoveryKeys: migrated {} collator(s) with hashed placeholder keys",
				num_keys,
			);
		});

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		if Self::was_executed() {
			return Ok(Vec::new());
		}

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
		// `pre_upgrade` returns no state when the migration was already executed.
		if state.is_empty() {
			return Ok(());
		}

		let pre_next_keys: Vec<(
			<R as pallet_session::Config>::ValidatorId,
			Translator::OldSessionKeys,
		)> = DecodeAll::decode_all(&mut &state[..]).map_err(|_| {
			sp_runtime::TryRuntimeError::Other("post_upgrade: invalid state encoding")
		})?;

		// Every ValidatorId from pre-upgrade must still have a NextKeys entry that
		// preserves the original aura key and carries the generated authority discovery key.
		for (id, pre_next_key) in &pre_next_keys {
			let aura = Translator::aura(&pre_next_key);
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
	use frame_support::{derive_impl, parameter_types, traits::ConstU64};
	use frame_system::pallet_prelude::HeaderFor;
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_core::H256;
	use sp_runtime::{
		traits::{Block as _, Header as _},
		BuildStorage, KeyTypeId,
	};

	fn new_test_ext() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
	}

	pub type AccountId = u64;

	parameter_types! {
		pub const Period: u64 = 10;
	}

	pub struct IdentityValidator;
	impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for IdentityValidator {
		fn convert(account: AccountId) -> Option<AccountId> {
			Some(account)
		}
	}

	type Block = sp_runtime::generic::Block<
		sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>,
		sp_runtime::generic::UncheckedExtrinsic<
			AccountId,
			RuntimeCall,
			sp_runtime::testing::UintAuthorityId,
			(),
		>,
	>;

	sp_runtime::impl_opaque_keys! {
		pub struct MockSessionKeys {
			pub aura: AuraId,
			pub authority_discovery: AuthorityDiscoveryId,
		}
	}

	impl_keys_translator!(MockSessionKeys, AuraId, AuthorityDiscoveryId);

	type Migration = AppendAuthorityDiscoveryKeys<Test, KeysTranslator>;

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Balances: pallet_balances,
			Session: pallet_session,
			MultiBlockMigrations: pallet_migrations,
		}
	);

	type Executive = frame_executive::Executive<
		Test,
		Block,
		frame_system::ChainContext<Test>,
		Test,
		AllPalletsWithSystem,
	>;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountData = pallet_balances::AccountData<u64>;
		type DbWeight = frame_support::weights::constants::RocksDbWeight;
		type SingleBlockMigrations = Migration;
		type MultiBlockMigrator = MultiBlockMigrations;
	}

	#[derive_impl(pallet_migrations::config_preludes::TestDefaultConfig)]
	impl pallet_migrations::Config for Test {
		type Migrations = ();
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
		type ShouldEndSession = pallet_session::PeriodicSessions<Period, ConstU64<0>>;
		type NextSessionRotation = pallet_session::PeriodicSessions<Period, ConstU64<0>>;
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

	/// The session keys `v` is expected to have after the migration.
	fn migrated_keys(v: AccountId) -> MockSessionKeys {
		let aura = aura_key(v);
		MockSessionKeys {
			authority_discovery: KeysTranslator::gen_authority_discovery(&aura),
			aura,
		}
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
			pallet_session::KeyOwner::<Test>::insert((AuraId::ID, aura.encode()), v);
		}
		sp_io::storage::set(&pallet_session::QueuedKeys::<Test>::hashed_key(), &queued.encode());
		pallet_session::Validators::<Test>::put(validators.to_vec());
	}

	/// Run the migration. When `try-runtime` is enabled also exercises the `pre_upgrade` →
	/// `on_runtime_upgrade` → `post_upgrade` hooks the try-runtime CLI runs in CI.
	fn run_migration() -> Weight {
		#[cfg(feature = "try-runtime")]
		let state = Migration::pre_upgrade().unwrap();

		let weight = Migration::on_runtime_upgrade();

		#[cfg(feature = "try-runtime")]
		Migration::post_upgrade(state).unwrap();

		weight
	}

	#[test]
	fn appends_placeholder_authority_discovery_keys() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2, 3];
			seed_old_layout(&validators);

			run_migration();

			for v in validators {
				assert_eq!(pallet_session::NextKeys::<Test>::get(v), Some(migrated_keys(v)));
				assert_eq!(
					pallet_session::KeyOwner::<Test>::get((AuraId::ID, aura_key(v).encode())),
					Some(v),
				);
				// Each validator has its own `KeyOwner` entry for its aura-derived
				// authority-discovery placeholder, so there is no last-writer-wins collision.
				assert_eq!(
					pallet_session::KeyOwner::<Test>::get((
						AuthorityDiscoveryId::ID,
						migrated_keys(v).authority_discovery.encode(),
					)),
					Some(v),
				);
			}
			assert_eq!(
				pallet_session::QueuedKeys::<Test>::get(),
				validators.map(|v| (v, migrated_keys(v))).to_vec()
			);
		});
	}

	#[test]
	fn runs_only_once() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2];
			seed_old_layout(&validators);

			let db = <Test as frame_system::Config>::DbWeight::get();
			assert_eq!(run_migration(), db.reads_writes(4, 8));
			assert!(Migration::was_executed());

			// A second run must not touch the (already migrated) keys again.
			let next_keys = pallet_session::NextKeys::<Test>::iter().collect::<Vec<_>>();
			let queued_keys = pallet_session::QueuedKeys::<Test>::get();
			assert_eq!(run_migration(), db.reads(1));
			assert_eq!(pallet_session::NextKeys::<Test>::iter().collect::<Vec<_>>(), next_keys);
			assert_eq!(pallet_session::QueuedKeys::<Test>::get(), queued_keys);
		});
	}

	#[test]
	fn works_with_empty_session_storage() {
		new_test_ext().execute_with(|| {
			let db = <Test as frame_system::Config>::DbWeight::get();
			assert_eq!(run_migration(), db.reads_writes(2, 2));
			assert!(Migration::was_executed());
			assert_eq!(pallet_session::NextKeys::<Test>::iter().count(), 0);
		});
	}

	/// Author an empty block on top of the current state, then import it with
	/// `Executive::execute_block`, like a node does.
	///
	/// Authoring follows the `BlockBuilder` runtime API (`initialize_block` + `finalize_block`)
	/// inside a storage transaction that is rolled back; this only computes a header with the
	/// correct state root. Importing then runs the full `execute_block` logic on the original
	/// state, including its `final_checks` against that header.
	fn run_block(number: u64, parent_hash: H256) -> H256 {
		let header = HeaderFor::<Test>::new(
			number,
			Default::default(),
			Default::default(),
			parent_hash,
			Default::default(),
		);

		sp_io::storage::start_transaction();
		Executive::initialize_block(&header);
		let header = Executive::finalize_block();
		sp_io::storage::rollback_transaction();

		Executive::execute_block(Block::new(header.clone(), vec![]).into());
		header.hash()
	}

	/// Enact a runtime upgrade at `upgrade_block` and keep producing blocks until `last_block`.
	fn upgrade_and_run_blocks(upgrade_block: u64, last_block: u64) {
		// Make `Executive` think the runtime code changed, so it runs `on_runtime_upgrade`.
		frame_system::LastRuntimeUpgrade::<Test>::kill();
		frame_system::Pallet::<Test>::set_block_number(upgrade_block - 1);
		let mut parent_hash = H256::default();
		for number in upgrade_block..=last_block {
			parent_hash = run_block(number, parent_hash);
		}
	}

	/// Upgrade exactly at a session boundary: `Executive` runs `on_runtime_upgrade` before
	/// `on_initialize`, so the rotation done by `pallet_session` in `on_initialize` already reads
	/// `QueuedKeys` and `NextKeys` in the new layout.
	#[test]
	fn upgrade_at_session_boundary_keeps_validators() {
		new_test_ext().execute_with(|| {
			let validators = [1, 2, 3];
			seed_old_layout(&validators);

			upgrade_and_run_blocks(Period::get(), Period::get() + 1);

			assert_eq!(pallet_session::CurrentIndex::<Test>::get(), 1);
			assert!(Migration::was_executed());

			assert_eq!(pallet_session::Validators::<Test>::get(), validators.to_vec());
			assert_eq!(
				pallet_session::QueuedKeys::<Test>::get(),
				validators.map(|v| (v, migrated_keys(v))).to_vec()
			);
			for v in validators {
				assert_eq!(pallet_session::NextKeys::<Test>::get(v), Some(migrated_keys(v)));
			}
		});
	}
}
