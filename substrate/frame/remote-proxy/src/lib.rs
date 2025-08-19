// Copyright (C) Polkadot Fellows.
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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Remote proxy pallet
//!
//! The pallet provides the functionality for using a proxy on a remote chain. The exact remote
//! location of the proxy depends on the [`RemoteProxyInterface`] implementation provided to this
//! pallet. The underlying implementation works by verifying proofs from the remote location that
//! prove the existence of a proxy. The remote proof is verified against a storage root from the
//! remote location. These storage roots are extracted from the relay chain. So, the security
//! of the proxy depends on the remote location. This means that the remote location should be a
//! trusted chain that for example doesn't create fake proxies.
//!
//! ## Functions
//!
//! The pallet provides the following functions:
//!
//! - [`Pallet::remote_proxy`]: Dispatch a wrapped call using the given proof over the existence of
//!   a remote proxy.
//!
//! - [`Pallet::register_remote_proxy_proof`]: Register the given `proof` in the current dispatch.
//!
//! - [`Pallet::remote_proxy_with_registered_proof`]: Use a previously registered `proof` to
//!   dispatch the wrapped call.
//!
//! ## Security considerations
//!
//! As explained above the security of the proxy depends on the remote location. So, if the remote
//! location is not trusted, it should not be configured as remote location. When configuring
//! [`MaxStorageRootsToKeep`](Config::MaxStorageRootsToKeep) it should be considered that the
//! lifetime of a proxy will be [`MaxStorageRootsToKeep`](Config::MaxStorageRootsToKeep) in the
//! past. This means when deleting a proxy at the remote location at X, it will take
//! [`MaxStorageRootsToKeep`](Config::MaxStorageRootsToKeep) time until the proxy can not be used
//! anymore. The reason for this is that the caller will be able to provide an old `proof` at which
//! the proxy was still available.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod tests;
mod weight;

use alloc::{boxed::Box, vec::Vec};
use codec::{Encode, MaxEncodedLen};
use frame::prelude::*;
use scale_info::TypeInfo;

pub use cumulus_primitives_core::PersistedValidationData;
use frame::{
	deps::{
		frame_support::{dispatch_context, StorageHasher},
		sp_core::Hasher,
	},
	prelude::storage::storage_prefix,
};
pub use pallet::*;
pub use pallet_proxy::ProxyDefinition;
pub use weight::WeightInfo;

/// The remote proxy interface.
pub trait RemoteProxyInterface<AccountId, ProxyType, BlockNumber> {
	/// The remote account id.
	type RemoteAccountId: Parameter + MaxEncodedLen;
	/// The remote proxy type.
	type RemoteProxyType: Parameter + MaxEncodedLen;
	/// The remote block number.
	type RemoteBlockNumber: Parameter
		+ Saturating
		+ MaxEncodedLen
		+ Default
		+ PartialOrd
		+ Ord
		+ From<u32>;
	/// The hash type used by the remote chain.
	type RemoteHash: Parameter + MaxEncodedLen;
	/// The hasher used by the remote chain.
	type RemoteHasher: Hasher<Out = Self::RemoteHash>;

	/// Get the latest block to storage root mapping.
	fn block_to_storage_root(
		validation_data: &PersistedValidationData,
	) -> Option<(Self::RemoteBlockNumber, <Self::RemoteHasher as Hasher>::Out)>;

	/// The storage key where to find the [`ProxyDefinition`] for the given proxy account in the
	/// remote chain.
	fn proxy_definition_storage_key(proxy: &Self::RemoteAccountId) -> Vec<u8> {
		let mut key = storage_prefix(b"Proxy", b"Proxies").to_vec();
		proxy.using_encoded(|p| {
			key.extend(Twox64Concat::hash(p));
		});
		key
	}

	/// Convert the local account id to the remote account id.
	///
	/// If the conversion is not possible, return `None`.
	fn local_to_remote_account_id(local: &AccountId) -> Option<Self::RemoteAccountId>;

	/// Convert the remote proxy definition to the local proxy definition.
	///
	/// If the conversion is not possible, return `None`.
	fn remote_to_local_proxy_defintion(
		remote: ProxyDefinition<
			Self::RemoteAccountId,
			Self::RemoteProxyType,
			Self::RemoteBlockNumber,
		>,
	) -> Option<ProxyDefinition<AccountId, ProxyType, BlockNumber>>;

	/// Create a remote proxy proof to be used in benchmarking.
	///
	/// Returns the `proof`, `block_number` and `storage_root`. The later are required to validate
	/// the `proof`.
	#[cfg(feature = "runtime-benchmarks")]
	fn create_remote_proxy_proof(
		caller: &AccountId,
		proxy: &AccountId,
	) -> (RemoteProxyProof<Self::RemoteBlockNumber>, Self::RemoteBlockNumber, Self::RemoteHash);
}

#[frame::pallet]
pub mod pallet {
	use super::*;
	use cumulus_pallet_parachain_system::OnSystemEvent;
	use cumulus_primitives_core::PersistedValidationData;

	type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

	pub(crate) type RemoteBlockNumberOf<T, I> =
		<<T as Config<I>>::RemoteProxy as RemoteProxyInterface<
			<T as frame_system::Config>::AccountId,
			<T as pallet_proxy::Config>::ProxyType,
			BlockNumberFor<T>,
		>>::RemoteBlockNumber;
	type RemoteAccountIdOf<T, I> = <<T as Config<I>>::RemoteProxy as RemoteProxyInterface<
		<T as frame_system::Config>::AccountId,
		<T as pallet_proxy::Config>::ProxyType,
		BlockNumberFor<T>,
	>>::RemoteAccountId;
	type RemoteHasherOf<T, I> = <<T as Config<I>>::RemoteProxy as RemoteProxyInterface<
		<T as frame_system::Config>::AccountId,
		<T as pallet_proxy::Config>::ProxyType,
		BlockNumberFor<T>,
	>>::RemoteHasher;
	type RemoteHashOf<T, I> = <<T as Config<I>>::RemoteProxy as RemoteProxyInterface<
		<T as frame_system::Config>::AccountId,
		<T as pallet_proxy::Config>::ProxyType,
		BlockNumberFor<T>,
	>>::RemoteHash;
	type RemoteProxyTypeOf<T, I> = <<T as Config<I>>::RemoteProxy as RemoteProxyInterface<
		<T as frame_system::Config>::AccountId,
		<T as pallet_proxy::Config>::ProxyType,
		BlockNumberFor<T>,
	>>::RemoteProxyType;
	type WeightInfoOf<T, I> = <T as Config<I>>::WeightInfo;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	/// Stores the last [`Config::MaxStorageRootsToKeep`] block to storage root mappings of the
	/// target chain.
	#[pallet::storage]
	pub type BlockToRoot<T: Config<I>, I: 'static = ()> = StorageValue<
		_,
		BoundedVec<(RemoteBlockNumberOf<T, I>, RemoteHashOf<T, I>), T::MaxStorageRootsToKeep>,
		ValueQuery,
	>;

	/// Configuration trait.
	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_proxy::Config {
		/// The maximum number of storage roots to keep.
		///
		/// The storage roots are used to validate the remote proofs. The more we keep in storage,
		/// the older the proof can be. This is not only seen as a maximum number, but also as the
		/// maximum difference between the latest and the oldest storage root stored. This means
		/// that if the chain for example did not progress for `MaxStorageRootsToKeep` blocks, only
		/// the latest added storage root will be available for validating proofs.
		type MaxStorageRootsToKeep: Get<u32>;

		/// The interface for interacting with the remote proxy.
		type RemoteProxy: RemoteProxyInterface<
			Self::AccountId,
			Self::ProxyType,
			BlockNumberFor<Self>,
		>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	impl<T: Config<I>, I: 'static> OnSystemEvent for Pallet<T, I> {
		fn on_validation_data(validation_data: &PersistedValidationData) {
			let Some((block, hash)) = T::RemoteProxy::block_to_storage_root(validation_data) else {
				return;
			};

			// Update the block to root mappings.
			BlockToRoot::<T, I>::mutate(|roots| {
				let delete_up_to =
					block.clone().saturating_sub(T::MaxStorageRootsToKeep::get().into());

				while roots.first().is_some_and(|f| f.0 <= delete_up_to) {
					roots.remove(0);
				}

				// We always remove all the old items before, thus there should always be space in
				// the vector.
				if roots.last().map_or(true, |(b, _)| *b != block) {
					let _res = roots.try_push((block, hash));
					debug_assert!(_res.is_ok());
				}
			});
		}

		fn on_validation_code_applied() {}
	}

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T, I = ()> {
		/// The local account id could not converted to the remote account id.
		CouldNotConvertLocalToRemoteAccountId,
		/// The anchor block of the remote proof is unknown.
		UnknownProofAnchorBlock,
		/// The proxy definition could not be found in the proof.
		InvalidProof,
		/// Failed to decode the remote proxy definition from the proof.
		ProxyDefinitionDecodingFailed,
		/// Announcement, if made at all, was made too recently.
		Unannounced,
		/// Could not find any matching proxy definition in the proof.
		DidNotFindMatchingProxyDefinition,
		/// Proxy proof not registered.
		ProxyProofNotRegistered,
	}

	/// The remote proxy proof to prove the existence of a proxy account.
	#[derive(
		core::fmt::Debug, Clone, Decode, DecodeWithMemTracking, Encode, TypeInfo, PartialEq, Eq,
	)]
	pub enum RemoteProxyProof<RemoteBlockNumber> {
		/// Assumes the default proxy storage layout.
		RelayChain { proof: Vec<Vec<u8>>, block: RemoteBlockNumber },
	}

	/// The dispatch context to keep track of registered proofs.
	#[derive(Default)]
	pub(crate) struct RemoteProxyContext<RemoteBlockNumber> {
		/// The registered proofs.
		pub(crate) proofs: Vec<RemoteProxyProof<RemoteBlockNumber>>,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Dispatch the given `call` from an account that the sender is authorised on a remote
		/// chain.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// Parameters:
		/// - `real`: The account that the proxy will make a call on behalf of.
		/// - `force_proxy_type`: Specify the exact proxy type to be used and checked for this call.
		/// - `call`: The call to be made by the `real` account.
		/// - `proof`: The proof from the remote chain about the existence of the proxy.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let di = call.get_dispatch_info();
			(WeightInfoOf::<T, I>::remote_proxy()
				// AccountData for inner call origin accountdata.
				.saturating_add(T::DbWeight::get().reads_writes(1, 1))
				.saturating_add(di.call_weight),
			di.class)
		})]
		pub fn remote_proxy(
			origin: OriginFor<T>,
			real: AccountIdLookupOf<T>,
			force_proxy_type: Option<T::ProxyType>,
			call: Box<<T as pallet_proxy::Config>::RuntimeCall>,
			proof: RemoteProxyProof<RemoteBlockNumberOf<T, I>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let real = T::Lookup::lookup(real)?;

			Self::do_remote_proxy(who, real, force_proxy_type, *call, proof)
		}

		/// Register a given remote proxy proof in the current [`dispatch_context`].
		///
		/// The registered remote proof can then be used later in the same context to execute a
		/// remote proxy call. This is for example useful when having a multisig operation. The
		/// multisig call can use [`Self::remote_proxy_with_registered_proof`] to get an approval by
		/// the members of the multisig. The final execution of the multisig call should be at least
		/// a batch of `register_remote_proxy_proof` and the multisig call that uses
		/// `remote_proxy_with_registered_proof`. This way the final approver can use a recent proof
		/// to prove the existence of the remote proxy. Otherwise it would require the multisig
		/// members to approve the call in [`Config::MaxStorageRootsToKeep`] amount of time.
		///
		/// It is supported to register multiple proofs, but the proofs need to be consumed in the
		/// reverse order as they were registered. Basically this means last in, first out.
		///
		/// The [`dispatch_context`] spans the entire lifetime of a transaction and every call in
		/// the transaction gets access to the same context.
		///
		/// # Example
		///
		/// ```ignore
		/// batch([
		///     register_remote_proxy_proof,
		///     as_multisig(remote_proxy_with_registered_proof(transfer))
		/// ])
		/// ```
		///
		/// As `proofs` can not be verified indefinitely (the time the storage roots are stored is
		/// limited) this function provides the possibility to provide a "fresh proof" at time of
		/// dispatch. As in the example above, this could be useful for multisig operation that
		/// depend on multiple members to approve a certain action, which can take multiple days.
		#[pallet::call_index(1)]
		#[pallet::weight({(WeightInfoOf::<T, I>::register_remote_proxy_proof(), DispatchClass::Normal)})]
		pub fn register_remote_proxy_proof(
			origin: OriginFor<T>,
			proof: RemoteProxyProof<RemoteBlockNumberOf<T, I>>,
		) -> DispatchResult {
			ensure_signed(origin)?;

			dispatch_context::with_context::<RemoteProxyContext<RemoteBlockNumberOf<T, I>>, _>(
				|context| {
					context.or_default().proofs.push(proof);
				},
			);

			Ok(())
		}

		/// Dispatch the given `call` from an account that the sender is authorised on a remote
		/// chain.
		///
		/// The dispatch origin for this call must be _Signed_. The difference to
		/// [`Self::remote_proxy`] is that the proof nees to registered before using
		/// [`Self::register_remote_proxy_proof`] (see for more information).
		///
		/// Parameters:
		/// - `real`: The account that the proxy will make a call on behalf of.
		/// - `force_proxy_type`: Specify the exact proxy type to be used and checked for this call.
		/// - `call`: The call to be made by the `real` account.
		#[pallet::call_index(2)]
		#[pallet::weight({
			let di = call.get_dispatch_info();
			(WeightInfoOf::<T, I>::remote_proxy_with_registered_proof()
				// AccountData for inner call origin accountdata.
				.saturating_add(T::DbWeight::get().reads_writes(1, 1))
				.saturating_add(di.call_weight),
			di.class)
		})]
		pub fn remote_proxy_with_registered_proof(
			origin: OriginFor<T>,
			real: AccountIdLookupOf<T>,
			force_proxy_type: Option<T::ProxyType>,
			call: Box<<T as pallet_proxy::Config>::RuntimeCall>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let real = T::Lookup::lookup(real)?;

			let proof = dispatch_context::with_context::<
				RemoteProxyContext<RemoteBlockNumberOf<T, I>>,
				_,
			>(|context| context.or_default().proofs.pop())
			.flatten()
			.ok_or(Error::<T, I>::ProxyProofNotRegistered)?;

			Self::do_remote_proxy(who, real, force_proxy_type, *call, proof)
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		fn do_remote_proxy(
			who: T::AccountId,
			real: T::AccountId,
			force_proxy_type: Option<T::ProxyType>,
			call: <T as pallet_proxy::Config>::RuntimeCall,
			proof: RemoteProxyProof<RemoteBlockNumberOf<T, I>>,
		) -> DispatchResult {
			let Some(real_remote) = T::RemoteProxy::local_to_remote_account_id(&real) else {
				return Err(Error::<T, I>::CouldNotConvertLocalToRemoteAccountId.into());
			};

			let def = match proof {
				RemoteProxyProof::RelayChain { proof, block } => {
					let roots = BlockToRoot::<T, I>::get();

					let Ok(storage_root) = roots
						.binary_search_by(|(b, _)| b.cmp(&block))
						.map(|pos| roots[pos].1.clone())
					else {
						return Err(Error::<T, I>::UnknownProofAnchorBlock.into());
					};

					let key = T::RemoteProxy::proxy_definition_storage_key(&real_remote);

					let db =
						sp_trie::StorageProof::new(proof).into_memory_db::<RemoteHasherOf<T, I>>();
					let value = sp_trie::read_trie_value::<sp_trie::LayoutV1<_>, _>(
						&db,
						&storage_root,
						&key,
						None,
						None,
					)
					.ok()
					.flatten()
					.ok_or(Error::<T, I>::InvalidProof)?;

					let proxy_definitions = alloc::vec::Vec::<
						ProxyDefinition<
							RemoteAccountIdOf<T, I>,
							RemoteProxyTypeOf<T, I>,
							RemoteBlockNumberOf<T, I>,
						>,
					>::decode(&mut &value[..])
					.map_err(|_| Error::<T, I>::ProxyDefinitionDecodingFailed)?;

					let f = |x: &ProxyDefinition<
						T::AccountId,
						T::ProxyType,
						BlockNumberFor<T>,
					>|
					 -> bool {
						x.delegate == who &&
							force_proxy_type.as_ref().is_none_or(|y| &x.proxy_type == y)
					};

					proxy_definitions
						.into_iter()
						.filter_map(T::RemoteProxy::remote_to_local_proxy_defintion)
						.find(f)
						.ok_or(Error::<T, I>::DidNotFindMatchingProxyDefinition)?
				},
			};

			ensure!(def.delay.is_zero(), Error::<T, I>::Unannounced);

			let proxy_def = pallet_proxy::ProxyDefinition {
				delegate: def.delegate,
				proxy_type: def.proxy_type,
				delay: Zero::zero(),
			};

			pallet_proxy::Pallet::<T>::do_proxy(proxy_def, real, call);

			Ok(())
		}
	}
}
