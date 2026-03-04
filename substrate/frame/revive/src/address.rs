// This file is part of Substrate.

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

//! Functions that deal contract addresses.

use crate::{BalanceOf, Config, Error, HoldReason, MappingDepositor, OriginalAccount, ensure};
use alloc::vec::Vec;
use codec::MaxEncodedLen;
use core::marker::PhantomData;
use frame_support::traits::{fungible::MutateHold, tokens::Precision};
use sp_core::{Get, H160};
use sp_io::hashing::keccak_256;
use sp_runtime::{AccountId32, DispatchResult, Saturating};

/// Map between the native chain account id `T` and an Ethereum [`H160`].
///
/// This trait exists only to emulate specialization for different concrete
/// native account ids. **Not** to make the mapping user configurable. Hence
/// the trait is `Sealed` and depending on your runtime configuration you need
/// to pick either [`AccountId32Mapper`] or [`H160Mapper`]. Picking the wrong
/// one will result in a compilation error. No footguns here.
///
/// Please note that we assume that the native account is at least 20 bytes and
/// only implement this type for a `T` where this is the case. Luckily, this is the
/// case for all existing runtimes as of right now. Reasoning is that this will allow
/// us to reverse an address -> account_id mapping by just stripping the prefix.
///
/// We require the mapping to be reversible. Since we are potentially dealing with types of
/// different sizes one direction of the mapping is necessarily lossy. This requires the mapping to
/// make use of the [`OriginalAccount`] storage item to reverse the mapping.
pub trait AddressMapper<T: Config>: private::Sealed {
	/// Convert an account id to an ethereum address.
	fn to_address(account_id: &T::AccountId) -> H160;

	/// Convert an ethereum address to a native account id.
	fn to_account_id(address: &H160) -> T::AccountId;

	/// Same as [`Self::to_account_id`] but always returns the fallback account.
	///
	/// This skips the query into [`OriginalAccount`] and always returns the stateless
	/// fallback account. This is useful when we know for a fact that the `address`
	/// in question is originally a `H160`. This is usually only the case when we
	/// generated a new contract address.
	fn to_fallback_account_id(address: &H160) -> T::AccountId;

	/// Create a stateful mapping for `account_id`
	///
	/// This will enable `to_account_id` to map back to the original
	/// `account_id` instead of the fallback account id.
	fn map(account_id: &T::AccountId) -> DispatchResult;

	/// Map an account id without taking any deposit.
	/// This is only useful for genesis configuration, or benchmarks.
	fn map_no_deposit(account_id: &T::AccountId) -> DispatchResult {
		Self::map(account_id)
	}

	/// Create a stateful mapping for `account_id`, holding the deposit from `depositor`
	/// instead of from `account_id` itself.
	///
	/// This is used by [`crate::Pallet::call_with_mappings`] to register a mapping on behalf
	/// of another account, with the caller (depositor) paying the storage deposit.
	///
	/// - Returns `Ok(())` without re-inserting if `account_id` is already correctly mapped —
	///   including eth-derived accounts that are implicitly mapped (idempotent, no deposit
	///   charged).
	/// - Returns [`crate::Error::MappingConflict`] if the derived H160 is already registered
	///   for a *different* native account.
	/// - When `depositor == account_id`, behaves identically to [`Self::map`]: the deposit is
	///   held from the mapped account under [`HoldReason::AddressMapping`] and no
	///   [`MappingDepositor`] entry is written.
	/// - Otherwise charges deposit (for both the [`OriginalAccount`] and [`MappingDepositor`]
	///   entries) from `depositor` under [`HoldReason::ExternalAddressMapping`].
	///
	/// The default implementation ignores `depositor` and delegates to [`Self::map`].
	/// Implementations that use [`OriginalAccount`] storage (i.e. [`AccountId32Mapper`])
	/// must override this.
	fn map_with_depositor(
		account_id: &T::AccountId,
		_depositor: &T::AccountId,
	) -> DispatchResult {
		Self::map(account_id)
	}

	/// Remove the mapping in order to reclaim the deposit.
	///
	/// There is no reason why one would unmap their `account_id` except
	/// for reclaiming the deposit.
	fn unmap(account_id: &T::AccountId) -> DispatchResult;

	/// Returns true if the `account_id` is usable as an origin.
	///
	/// This means either the `account_id` doesn't require a stateful mapping
	/// or a stateful mapping exists.
	fn is_mapped(account_id: &T::AccountId) -> bool;
}

mod private {
	pub trait Sealed {}
	impl<T> Sealed for super::AccountId32Mapper<T> {}
	impl<T> Sealed for super::H160Mapper<T> {}
	impl<T> Sealed for super::TestAccountMapper<T> {}
}

/// The mapper to be used if the account id is `AccountId32`.
///
/// It converts between addresses by either hash then truncate the last 12 bytes or
/// suffixing them. To recover the original account id of a hashed and truncated account id we use
/// [`OriginalAccount`] and will fall back to all `0xEE` if account was found. This means contracts
/// and plain wallets controlled by an `secp256k1` always have a `0xEE` suffixed account.
pub struct AccountId32Mapper<T>(PhantomData<T>);

/// The mapper to be used if the account id is `H160`.
///
/// It just trivially returns its inputs and doesn't make use of any state.
#[allow(dead_code)]
pub struct H160Mapper<T>(PhantomData<T>);

/// An account mapper that can be used for testing u64 account ids.
pub struct TestAccountMapper<T>(PhantomData<T>);

impl<T> AddressMapper<T> for AccountId32Mapper<T>
where
	T: Config<AccountId = AccountId32>,
{
	fn to_address(account_id: &AccountId32) -> H160 {
		let account_bytes: &[u8; 32] = account_id.as_ref();
		if is_eth_derived(account_id) {
			// this was originally an eth address
			// we just strip the 0xEE suffix to get the original address
			H160::from_slice(&account_bytes[..20])
		} else {
			// this is an (ed|sr)25510 derived address
			// avoid truncating the public key by hashing it first
			let account_hash = keccak_256(account_bytes);
			H160::from_slice(&account_hash[12..])
		}
	}

	fn to_account_id(address: &H160) -> AccountId32 {
		<OriginalAccount<T>>::get(address).unwrap_or_else(|| Self::to_fallback_account_id(address))
	}

	fn to_fallback_account_id(address: &H160) -> AccountId32 {
		let mut account_id = AccountId32::new([0xEE; 32]);
		let account_bytes: &mut [u8; 32] = account_id.as_mut();
		account_bytes[..20].copy_from_slice(address.as_bytes());
		account_id
	}

	fn map(account_id: &T::AccountId) -> DispatchResult {
		ensure!(!Self::is_mapped(account_id), <Error<T>>::AccountAlreadyMapped);

		// each mapping entry stores the address (20 bytes) and the account id (32 bytes)
		let deposit = T::DepositPerByte::get()
			.saturating_mul(52u32.into())
			.saturating_add(T::DepositPerItem::get());
		T::Currency::hold(&HoldReason::AddressMapping.into(), account_id, deposit)?;

		<OriginalAccount<T>>::insert(Self::to_address(account_id), account_id);
		Ok(())
	}

	fn map_no_deposit(account_id: &T::AccountId) -> DispatchResult {
		ensure!(!Self::is_mapped(account_id), <Error<T>>::AccountAlreadyMapped);
		<OriginalAccount<T>>::insert(Self::to_address(account_id), account_id);
		Ok(())
	}

	fn map_with_depositor(
		account_id: &T::AccountId,
		depositor: &T::AccountId,
	) -> DispatchResult {
		let address = Self::to_address(account_id);

		// If already mapped (eth-derived or via OriginalAccount), just verify
		// the mapping is consistent; no deposit is charged.
		if Self::is_mapped(account_id) {
			ensure!(
				Self::to_account_id(&address) == *account_id,
				<Error<T>>::MappingConflict
			);
			return Ok(());
		}

		// Self-mapping: depositor == account_id.  Behave identically to `map()`:
		// hold under AddressMapping, charge only for OriginalAccount (no MappingDepositor entry).
		if account_id == depositor {
			return Self::map(account_id);
		}

		// Third-party mapping: deposit held from depositor under ExternalAddressMapping.
		// Deposit covers:
		//   OriginalAccount entry : 20 (H160 key) + 32 (AccountId32 value) = 52 bytes
		//   MappingDepositor entry: 20 (H160 key) + sizeof(T::AccountId, BalanceOf<T>) bytes
		//
		// The exact sizes are determined at runtime via MaxEncodedLen so the calculation
		// stays correct regardless of the concrete AccountId or Balance types.
		let mapping_depositor_value_bytes =
			(T::AccountId::max_encoded_len() + BalanceOf::<T>::max_encoded_len()) as u32;
		let deposit = T::DepositPerByte::get()
			.saturating_mul((52u32 + 20u32 + mapping_depositor_value_bytes).into())
			.saturating_add(T::DepositPerItem::get().saturating_mul(2u32.into()));

		// Use a dedicated hold reason so that releasing this deposit later does not
		// accidentally release unrelated AddressMapping holds the depositor may hold
		// (e.g. from calling map_account for their own account).
		T::Currency::hold(&HoldReason::ExternalAddressMapping.into(), depositor, deposit)?;

		<OriginalAccount<T>>::insert(&address, account_id);
		<MappingDepositor<T>>::insert(&address, (depositor.clone(), deposit));
		Ok(())
	}

	fn unmap(account_id: &T::AccountId) -> DispatchResult {
		let address = Self::to_address(account_id);

		// If the mapping was funded by a third-party depositor (via `call_with_mappings`),
		// release exactly the recorded deposit from that depositor's ExternalAddressMapping hold.
		// Using the stored amount (not release_all) ensures that only this mapping's deposit
		// is released, leaving any other ExternalAddressMapping holds the depositor may have
		// for different mappings intact.
		if let Some((depositor, deposit)) = <MappingDepositor<T>>::take(&address) {
			T::Currency::release(
				&HoldReason::ExternalAddressMapping.into(),
				&depositor,
				deposit,
				Precision::Exact,
			)?;
			<OriginalAccount<T>>::remove(&address);
			return Ok(());
		}

		// Self-funded mapping: remove the entry and release the self-held deposit.
		<OriginalAccount<T>>::remove(&address);
		T::Currency::release_all(
			&HoldReason::AddressMapping.into(),
			account_id,
			Precision::BestEffort,
		)?;
		Ok(())
	}

	fn is_mapped(account_id: &T::AccountId) -> bool {
		is_eth_derived(account_id) ||
			<OriginalAccount<T>>::contains_key(Self::to_address(account_id))
	}
}

impl<T> AddressMapper<T> for TestAccountMapper<T>
where
	T: Config<AccountId = u64>,
{
	fn to_address(account_id: &T::AccountId) -> H160 {
		let mut bytes = [0u8; 20];
		bytes[12..].copy_from_slice(&account_id.to_be_bytes());
		H160::from(bytes)
	}

	fn to_account_id(address: &H160) -> T::AccountId {
		Self::to_fallback_account_id(address)
	}

	fn to_fallback_account_id(address: &H160) -> T::AccountId {
		u64::from_be_bytes(address.as_ref()[12..].try_into().unwrap())
	}

	fn map(_account_id: &T::AccountId) -> DispatchResult {
		Ok(())
	}

	fn unmap(_account_id: &T::AccountId) -> DispatchResult {
		Ok(())
	}

	fn is_mapped(_account_id: &T::AccountId) -> bool {
		true
	}
}

/// Returns true if the passed account id is controlled by an eth key.
///
/// This is a stateless check that just compares the last 12 bytes. Please note that it is
/// theoretically possible to create an ed25519 keypair that passed this filter. However,
/// this can't be used for an attack. It also won't happen by accident since everybody is using
/// sr25519 where this is not a valid public key.
pub fn is_eth_derived(account_id: &AccountId32) -> bool {
	let account_bytes: &[u8; 32] = account_id.as_ref();
	&account_bytes[20..] == &[0xEE; 12]
}

impl<T> AddressMapper<T> for H160Mapper<T>
where
	T: Config,
	crate::AccountIdOf<T>: AsRef<[u8; 20]> + From<H160>,
{
	fn to_address(account_id: &T::AccountId) -> H160 {
		H160::from_slice(account_id.as_ref())
	}

	fn to_account_id(address: &H160) -> T::AccountId {
		Self::to_fallback_account_id(address)
	}

	fn to_fallback_account_id(address: &H160) -> T::AccountId {
		(*address).into()
	}

	fn map(_account_id: &T::AccountId) -> DispatchResult {
		Ok(())
	}

	fn unmap(_account_id: &T::AccountId) -> DispatchResult {
		Ok(())
	}

	fn is_mapped(_account_id: &T::AccountId) -> bool {
		true
	}
}

/// Determine the address of a contract using CREATE semantics.
pub fn create1(deployer: &H160, nonce: u64) -> H160 {
	let mut list = rlp::RlpStream::new_list(2);
	list.append(&deployer.as_bytes());
	list.append(&nonce);
	let hash = keccak_256(&list.out());
	H160::from_slice(&hash[12..])
}

/// Determine the address of a contract using the CREATE2 semantics.
pub fn create2(deployer: &H160, code: &[u8], input_data: &[u8], salt: &[u8; 32]) -> H160 {
	let init_code_hash = {
		let init_code: Vec<u8> = code.into_iter().chain(input_data).cloned().collect();
		keccak_256(init_code.as_ref())
	};
	let mut bytes = [0; 85];
	bytes[0] = 0xff;
	bytes[1..21].copy_from_slice(deployer.as_bytes());
	bytes[21..53].copy_from_slice(salt);
	bytes[53..85].copy_from_slice(&init_code_hash);
	let hash = keccak_256(&bytes);
	H160::from_slice(&hash[12..])
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		AddressMapper, Error, MappingDepositor, OriginalAccount,
		test_utils::*,
		tests::{ExtBuilder, Test},
	};
	use frame_support::{
		assert_err,
		traits::fungible::{InspectHold, Mutate, MutateHold},
		traits::tokens::Precision,
	};
	use pretty_assertions::assert_eq;
	use sp_core::{H160, hex2array};

	#[test]
	fn create1_works() {
		assert_eq!(
			create1(&ALICE_ADDR, 1u64),
			H160(hex2array!("c851da37e4e8d3a20d8d56be2963934b4ad71c3b")),
		)
	}

	#[test]
	fn create2_works() {
		assert_eq!(
			create2(
				&ALICE_ADDR,
				&hex2array!("600060005560016000"),
				&hex2array!("55"),
				&hex2array!("1234567890123456789012345678901234567890123456789012345678901234")
			),
			H160(hex2array!("7f31e795e5836a19a8f919ab5a9de9a197ecd2b6")),
		)
	}

	#[test]
	fn fallback_map_works() {
		assert!(<Test as Config>::AddressMapper::is_mapped(&ALICE));
		assert_eq!(
			ALICE_FALLBACK,
			<Test as Config>::AddressMapper::to_fallback_account_id(&ALICE_ADDR)
		);
		assert_eq!(ALICE_ADDR, <Test as Config>::AddressMapper::to_address(&ALICE_FALLBACK));
	}

	#[test]
	fn map_works() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&EVE, 1_000_000);
			// before mapping the fallback account is returned
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(EVE_FALLBACK, <Test as Config>::AddressMapper::to_account_id(&EVE_ADDR));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				),
				0
			);

			// when mapped the full account id is returned
			<Test as Config>::AddressMapper::map(&EVE).unwrap();
			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(EVE, <Test as Config>::AddressMapper::to_account_id(&EVE_ADDR));
			assert!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				) > 0
			);
		});
	}

	#[test]
	fn map_fallback_account_fails() {
		ExtBuilder::default().build().execute_with(|| {
			assert!(<Test as Config>::AddressMapper::is_mapped(&ALICE));
			// alice is an e suffixed account and hence cannot be mapped
			assert_err!(
				<Test as Config>::AddressMapper::map(&ALICE),
				<Error<Test>>::AccountAlreadyMapped,
			);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&ALICE
				),
				0
			);
		});
	}

	#[test]
	fn double_map_fails() {
		ExtBuilder::default().build().execute_with(|| {
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			<Test as Config>::Currency::set_balance(&EVE, 1_000_000);
			<Test as Config>::AddressMapper::map(&EVE).unwrap();
			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));
			let deposit = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::AddressMapping.into(),
				&EVE,
			);
			assert_err!(
				<Test as Config>::AddressMapper::map(&EVE),
				<Error<Test>>::AccountAlreadyMapped,
			);
			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				),
				deposit
			);
		});
	}

	#[test]
	fn unmap_works() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&EVE, 1_000_000);
			<Test as Config>::AddressMapper::map(&EVE).unwrap();
			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				) > 0
			);

			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				),
				0
			);

			// another unmap is a noop
			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE
				),
				0
			);
		});
	}

	#[test]
	fn map_with_depositor_works() {
		ExtBuilder::default().build().execute_with(|| {
			// ALICE is the depositor (eth-derived, needs funds to pay the deposit)
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&ALICE,
				),
				0
			);

			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();

			// EVE is now mapped
			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(<Test as Config>::AddressMapper::to_account_id(&EVE_ADDR), EVE);

			// Deposit is held from ALICE (depositor) under ExternalAddressMapping, not from EVE
			assert!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				) > 0
			);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE,
				),
				0
			);

			// MappingDepositor storage records ALICE as the depositor (with deposit amount)
			assert_eq!(
				MappingDepositor::<Test>::get(&EVE_ADDR).map(|(a, _)| a),
				Some(ALICE)
			);
		});
	}

	#[test]
	fn map_with_depositor_idempotent_for_eth_derived() {
		ExtBuilder::default().build().execute_with(|| {
			// BOB is the prospective depositor, but no deposit should be taken
			<Test as Config>::Currency::set_balance(&BOB, 1_000_000);

			// ALICE is eth-derived: is_mapped returns true without OriginalAccount
			assert!(<Test as Config>::AddressMapper::is_mapped(&ALICE));

			// map_with_depositor is a no-op: no deposit charged
			<Test as Config>::AddressMapper::map_with_depositor(&ALICE, &BOB).unwrap();

			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&BOB,
				),
				0
			);
			// No MappingDepositor entry was created
			assert!(MappingDepositor::<Test>::get(&ALICE_ADDR).is_none());
		});
	}

	#[test]
	fn map_with_depositor_idempotent_when_already_mapped() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// First mapping: ALICE pays deposit
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();
			let deposit = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ExternalAddressMapping.into(),
				&ALICE,
			);
			assert!(deposit > 0);

			// Second call: idempotent, no additional deposit taken
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				deposit
			);
		});
	}

	#[test]
	fn map_with_depositor_conflict_fails() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// Manually plant a conflicting mapping: EVE_ADDR -> ALICE (not EVE)
			OriginalAccount::<Test>::insert(EVE_ADDR, ALICE);

			// map_with_depositor for EVE fails: EVE_ADDR already maps to a different account
			assert_err!(
				<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE),
				<Error<Test>>::MappingConflict,
			);

			// No deposit was taken
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				0
			);
		});
	}

	#[test]
	fn unmap_releases_depositor() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// Map EVE with ALICE as depositor
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();
			let deposit = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ExternalAddressMapping.into(),
				&ALICE,
			);
			assert!(deposit > 0);

			// EVE unmaps: the ExternalAddressMapping hold is released back to ALICE, not EVE
			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();

			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				0
			);
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE,
				),
				0
			);
			// MappingDepositor entry is cleaned up
			assert!(MappingDepositor::<Test>::get(&EVE_ADDR).is_none());
		});
	}


	/// ALICE pays for EVE's mapping and then for a second account's mapping (FRANK).
	/// Unmapping one must not touch the deposit held for the other.
	#[test]
	fn depositor_multiple_mappings_independent() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// A second non-eth-derived account so it also needs a stateful mapping.
			let frank = AccountId32::new([6u8; 32]);

			// ALICE pays deposits for both EVE and FRANK.
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();
			<Test as Config>::AddressMapper::map_with_depositor(&frank, &ALICE).unwrap();

			let total_held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ExternalAddressMapping.into(),
				&ALICE,
			);
			// Both mappings cost the same deposit D in a single test run.
			let d = total_held / 2;
			assert!(d > 0);
			assert_eq!(total_held, 2 * d);

			// Unmapping EVE releases exactly D (EVE's deposit) and leaves FRANK's untouched.
			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();

			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert!(<Test as Config>::AddressMapper::is_mapped(&frank));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				d
			);

			// Unmapping FRANK releases the remaining D.
			<Test as Config>::AddressMapper::unmap(&frank).unwrap();

			assert!(!<Test as Config>::AddressMapper::is_mapped(&frank));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				0
			);
		});
	}

	/// Verifies that `unmap` releases the **exact** stored deposit even when two mappings were
	/// registered with different deposit amounts (as would happen if `DepositPerByte` or
	/// `DepositPerItem` changed between the two `call_with_mappings` calls).
	///
	/// The second mapping is constructed directly via storage manipulation so that its stored
	/// deposit amount differs from the first mapping's amount.
	#[test]
	fn depositor_multiple_mappings_different_amounts() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 1_000_000);

			// First mapping (normal path): ALICE pays deposit D1 for EVE.
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &ALICE).unwrap();
			let d1 = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::ExternalAddressMapping.into(),
				&ALICE,
			);
			assert!(d1 > 0);

			// Second mapping: simulate a registration that happened when the deposit config was
			// different. We construct the state manually so D2 != D1.
			let frank = AccountId32::new([6u8; 32]);
			let frank_addr = <Test as Config>::AddressMapper::to_address(&frank);
			let d2 = d1 + 7; // arbitrary "different past deposit"

			<Test as Config>::Currency::hold(
				&HoldReason::ExternalAddressMapping.into(),
				&ALICE,
				d2,
			)
			.unwrap();
			OriginalAccount::<Test>::insert(frank_addr, frank.clone());
			MappingDepositor::<Test>::insert(frank_addr, (ALICE.clone(), d2));

			// Total held from ALICE is exactly D1 + D2.
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				d1 + d2
			);

			// Unmapping EVE releases exactly D1; D2 must remain held for FRANK.
			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert!(<Test as Config>::AddressMapper::is_mapped(&frank));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				d2
			);

			// Unmapping FRANK releases exactly D2; nothing should be left.
			<Test as Config>::AddressMapper::unmap(&frank).unwrap();
			assert!(!<Test as Config>::AddressMapper::is_mapped(&frank));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&ALICE,
				),
				0
			);
		});
	}

	/// When `depositor == account_id`, `map_with_depositor` must behave exactly like `map`:
	/// deposit held under `AddressMapping` (not `ExternalAddressMapping`), no `MappingDepositor`
	/// entry written, and `unmap` releases it back to the mapped account.
	#[test]
	fn map_with_depositor_self_behaves_like_map() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&EVE, 1_000_000);

			// EVE maps herself via map_with_depositor with depositor == account_id.
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &EVE).unwrap();

			assert!(<Test as Config>::AddressMapper::is_mapped(&EVE));

			// Deposit is held under AddressMapping (self-funded), just like map().
			let held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::AddressMapping.into(),
				&EVE,
			);
			assert!(held > 0);

			// No ExternalAddressMapping hold and no MappingDepositor entry.
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::ExternalAddressMapping.into(),
					&EVE,
				),
				0
			);
			assert!(MappingDepositor::<Test>::get(&EVE_ADDR).is_none());

			// Unmapping releases the deposit back to EVE.
			<Test as Config>::AddressMapper::unmap(&EVE).unwrap();
			assert!(!<Test as Config>::AddressMapper::is_mapped(&EVE));
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE,
				),
				0
			);
		});
	}

	/// Idempotency: calling `map_with_depositor(EVE, EVE)` when EVE is already self-mapped
	/// (via `map_account`) returns Ok without charging an extra deposit or erroring.
	#[test]
	fn map_with_depositor_self_idempotent_when_already_self_mapped() {
		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&EVE, 1_000_000);

			// EVE self-maps normally first.
			<Test as Config>::AddressMapper::map(&EVE).unwrap();
			let held = <Test as Config>::Currency::balance_on_hold(
				&HoldReason::AddressMapping.into(),
				&EVE,
			);
			assert!(held > 0);

			// Second call with depositor == account_id is idempotent: no extra deposit.
			<Test as Config>::AddressMapper::map_with_depositor(&EVE, &EVE).unwrap();
			assert_eq!(
				<Test as Config>::Currency::balance_on_hold(
					&HoldReason::AddressMapping.into(),
					&EVE,
				),
				held
			);
			assert!(MappingDepositor::<Test>::get(&EVE_ADDR).is_none());
		});
	}

}
