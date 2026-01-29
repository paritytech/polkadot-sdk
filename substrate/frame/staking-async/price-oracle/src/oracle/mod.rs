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

// TODO: why time is the way it is.

pub mod offchain;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod test;

// re-export all pallet parts, needed for runtime macros to work.
pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::{offchain, WeightInfo};
	use alloc::vec::Vec;
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::*,
		traits::{Defensive, DefensiveTruncateInto, EnsureOrigin, OneSessionHandler, Time},
		Parameter,
	};
	use frame_system::{
		offchain::{AppCrypto, CreateBare, CreateSignedTransaction},
		pallet_prelude::*,
	};
	use sp_runtime::{
		traits::{BlockNumberProvider, Member},
		FixedU128, Percent, RuntimeAppPublic, Saturating,
	};

	/// Alias for the moment type.
	pub type MomentOf<T> = <<T as Config>::TimeProvider as Time>::Moment;

	/// Alias for the price data type.
	pub type PriceDataOf<T> = PriceData<BlockNumberFor<T>, MomentOf<T>>;

	/// The error type that an implementation of [`Tally`] can return.
	///
	/// The actual error is generic; this enum is just distinguishing whether because of this error
	/// we should keep the old votes, or yank them.
	#[derive(Clone, PartialEq, Eq, Debug)]
	pub enum TallyOuterError<Error> {
		/// An error happened, and we should yank existing votes as they are not useful anymore.
		YankVotes(Error),
		/// An error happened, but we can keep the old votes as they are useful.
		///
		/// Note that this keeps the votes iff they still respect [`Config::MaxVoteAge`].
		KeepVotes(Error),
	}

	/// Interface to be implemented by the tally algorithm that we intend to use here.
	pub trait Tally {
		/// The asset-id type.
		type AssetId;
		/// The account-id type.
		type AccountId;
		/// The block number type.
		type BlockNumber;
		/// The error type.
		type Error: Debug + Eq + PartialEq + Clone;

		/// Tally the votes for a given asset.
		///
		/// The vote argument is a vector of (account-id, vote-price-value, produced-in) tuples.
		fn tally(
			asset_id: Self::AssetId,
			votes: Vec<(Self::AccountId, FixedU128, Self::BlockNumber)>,
		) -> Result<(FixedU128, Percent), TallyOuterError<Self::Error>>;
	}

	/// Listener hook to be implemented by entities that wish to be informed of price updates.
	#[impl_trait_for_tuples::impl_for_tuples(8)]
	pub trait OnPriceUpdate<AssetId, BlockNumber, Moment> {
		fn on_price_update(asset_id: AssetId, new: PriceData<BlockNumber, Moment>);
	}

	#[pallet::config]
	pub trait Config:
		frame_system::Config + CreateSignedTransaction<Call<Self>> + CreateBare<Call<Self>>
	{
		/// The key type for the session key we use to sign [`Call::vote`].
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>
			+ RuntimeAppPublic
			+ Parameter
			+ Member
			+ MaxEncodedLen;

		/// Maximum number of authorities that we can accept.
		///
		/// This is only used to bound data-types, and should always be an upper bound on the
		/// validator set size of the relay chain.
		type MaxAuthorities: Get<u32>;

		/// The type of the identifier of other assets, the price of which we are tracking
		/// against DOT.
		type AssetId: Member + Parameter + MaybeSerializeDeserialize + MaxEncodedLen + Copy;

		/// Maximum number of endpoints that can be added to an asset.
		type MaxEndpointsPerAsset: Get<u32>;

		/// Maximum byte-size of an endpoint (string length).
		type MaxEndpointLength: Get<u32>;

		/// The number of previous price and vote data-points to keep onchain.
		type HistoryDepth: Get<u32>;

		/// Maximum number of votes that can be submitted per block.
		///
		/// This is merely an upper bound on the number of votes that can be submitted. It doesn't
		/// mean that all of these votes are used for tallying.
		type MaxVotesPerBlock: Get<u32>;

		/// The maximum age of the [`Pallet::vote`] call.
		///
		/// Note that this value is treated at face-value and is based on the validators running the
		/// exact code provided by the [`crate::offchain`] machinery.
		type MaxVoteAge: Get<BlockNumberFor<Self>>;

		/// The tally manager to use.
		type TallyManager: Tally<
			AssetId = Self::AssetId,
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
		>;

		/// Type providing the relay block-number value.
		type RelayBlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// Type providing a secure notion of timestamp.
		type TimeProvider: Time;

		/// Hook to inform other systems that the price has been updated.
		///
		/// Is essentially a listener for [`Price`] storage item.
		type OnPriceUpdate: OnPriceUpdate<Self::AssetId, BlockNumberFor<Self>, MomentOf<Self>>;

		/// Every `PriceUpdateInterval` blocks, the offchain worker will submit a price update
		/// transaction.
		type PriceUpdateInterval: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: super::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new set of validators was announced.
		NewValidatorsAnnounced { count: u32 },
		/// A price vote was submitted.
		VoteSubmitted { who: T::AccountId, asset_id: T::AssetId, price: FixedU128 },
		/// Price was updated after tallying votes.
		PriceUpdated {
			asset_id: T::AssetId,
			old_price: FixedU128,
			new_price: FixedU128,
			vote_count: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The asset id was not found -- is not being tracked yet.
		AssetNotTracked,
		/// The asset is already being tracked.
		AssetAlreadyTracked,
		/// The number of votes for an asset has exceeded the maximum allowed per block.
		///
		/// See [`Config::MaxVotesPerBlock`].
		TooManyVotes,
		/// The bump price call is too old.
		///
		/// See [`Config::MaxVoteAge`].
		OldVote,
		/// Too many endpoints for an asset.
		///
		/// See [`Config::MaxEndpointsPerAsset`].
		TooManyEndpoints,
		/// The endpoint was not found.
		EndpointNotFound,
	}

	/// Current best known authorities.
	///
	/// Stored value is `(who, confidence)`.
	#[pallet::storage]
	pub type Authorities<T: Config> =
		StorageValue<_, BoundedVec<(T::AccountId, Percent), T::MaxAuthorities>, ValueQuery>;

	/// Wrapper struct managing the price-related storage items in this pallet.
	pub(crate) struct StorageManager<T: Config>(core::marker::PhantomData<T>);

	impl<T: Config> StorageManager<T> {
		/// Current best price of an asset.
		pub(crate) fn current_price(
			asset_id: T::AssetId,
		) -> Option<PriceData<BlockNumberFor<T>, MomentOf<T>>> {
			Price::<T>::get(&asset_id)
		}

		/// All of the assets that we are tracking and their list of feeds.
		pub(crate) fn tracked_assets_with_feeds() -> Vec<(T::AssetId, Vec<Vec<u8>>)> {
			Endpoints::<T>::iter()
				.map(|(asset_id, endpoints)| {
					(asset_id, endpoints.into_inner().into_iter().map(|e| e.into_inner()).collect())
				})
				.collect()
		}

		/// All of the assets that we are tracking.
		pub(crate) fn tracked_assets() -> Vec<T::AssetId> {
			Endpoints::<T>::iter_keys().collect()
		}

		/// Register a new asset to be tracked.
		fn register_asset(
			asset_id: T::AssetId,
			endpoints: BoundedVec<BoundedVec<u8, T::MaxEndpointLength>, T::MaxEndpointsPerAsset>,
		) -> DispatchResult {
			ensure!(!Self::is_tracked(asset_id), Error::<T>::AssetAlreadyTracked);
			Endpoints::<T>::insert(asset_id, endpoints);
			Ok(())
		}

		/// Deregister an asset from being tracked.
		fn deregister_asset(asset_id: T::AssetId) -> DispatchResult {
			ensure!(Self::is_tracked(asset_id), Error::<T>::AssetNotTracked);
			Endpoints::<T>::remove(asset_id);
			Price::<T>::remove(asset_id);
			PriceHistory::<T>::remove(asset_id);
			// Note: Safe because we are deleting at most `ConfigHistoryDepth` keys here.
			let cleared = BlockVotes::<T>::clear_prefix(asset_id, u32::MAX, None);
			debug_assert!(cleared.maybe_cursor.is_none(), "should clear all votes");
			Ok(())
		}

		/// Add an endpoint to an already tracked asset.
		fn add_endpoint(
			asset_id: T::AssetId,
			endpoint: BoundedVec<u8, T::MaxEndpointLength>,
		) -> DispatchResult {
			let mut stored = Endpoints::<T>::get(&asset_id).ok_or(Error::<T>::AssetNotTracked)?;
			stored.try_push(endpoint).map_err(|_| Error::<T>::TooManyEndpoints)?;
			Endpoints::<T>::insert(asset_id, stored);
			Ok(())
		}

		/// Remove an endpoint from an already tracked asset.
		fn remove_endpoint_at(asset_id: T::AssetId, index: usize) -> DispatchResult {
			let mut stored = Endpoints::<T>::get(&asset_id).ok_or(Error::<T>::AssetNotTracked)?;
			ensure!(index < stored.len(), Error::<T>::EndpointNotFound);
			let _removed = stored.remove(index);
			Endpoints::<T>::insert(asset_id, stored);
			Ok(())
		}

		/// Canonical notion of whether an asset is tracked or not.
		fn is_tracked(asset_id: T::AssetId) -> bool {
			Endpoints::<T>::contains_key(asset_id)
		}

		/// Add a new `vote` or `asset_id` from `who`
		fn add_vote(
			asset_id: T::AssetId,
			who: T::AccountId,
			vote: Vote<BlockNumberFor<T>>,
		) -> DispatchResult {
			ensure!(Self::is_tracked(asset_id), Error::<T>::AssetNotTracked);

			let now = Pallet::<T>::local_block_number();
			let mut votes = BlockVotes::<T>::get(asset_id, Pallet::<T>::local_block_number());
			votes.try_insert(who.clone(), vote).map_err(|_| Error::<T>::TooManyVotes)?;
			BlockVotes::<T>::insert(asset_id, now, votes);

			Ok(())
		}

		/// Update the price of an asset. This will:
		///
		/// * Store the new price in [`Price`].
		/// * Append the current price to the price history in [`PriceHistory`], removing stale ones
		///   if necessary.
		/// * Removes stale votes from [`BlockVotes`] if necessary.
		/// * Returns the new price.
		fn update(
			asset_id: T::AssetId,
			price: FixedU128,
			confidence: Percent,
			local_block_number: BlockNumberFor<T>,
		) -> Result<PriceData<BlockNumberFor<T>, MomentOf<T>>, Error<T>> {
			// ensure this asset is tracked at this point.
			ensure!(Self::is_tracked(asset_id), Error::<T>::AssetNotTracked);

			// Grab price related data.
			let maybe_yanked_price = Price::<T>::take(asset_id);
			let updated_in = TimePoint {
				local: Pallet::<T>::local_block_number(),
				relay: Pallet::<T>::relay_block_number(),
				timestamp: T::TimeProvider::now(),
			};
			let new_price = PriceData { price, confidence, updated_in };

			// Update the new price.
			Price::<T>::insert(asset_id, &new_price);

			// If history is to be kept, yank old historical data.
			if !T::HistoryDepth::get().is_zero() {
				if let Some(yanked_price) = maybe_yanked_price {
					let mut price_history = PriceHistory::<T>::get(asset_id);
					if price_history.is_full() {
						price_history.remove(0);
					}
					let _ = price_history
						.try_push(yanked_price)
						.defensive_proof("is not full; try_push will not fail; qed");
					PriceHistory::<T>::insert(asset_id, price_history);
				}

				// Remove stale voting data.
				if let Some(to_remove) = Pallet::<T>::local_block_number()
					.checked_sub(&(T::HistoryDepth::get().saturating_add(1).into()))
				{
					// Note: `T::HistoryDepth::get().saturating_add(1)` because we want to keep
					// `HistoryDepth` old price voting data, on top of the current
					// price/voting-data.
					BlockVotes::<T>::remove(&asset_id, to_remove);
				}
			} else {
				// yank current current vote.
				BlockVotes::<T>::remove(&asset_id, local_block_number);
			}

			Ok(new_price)
		}
	}

	#[cfg(any(test, feature = "std", feature = "try-runtime"))]
	impl<T: Config> StorageManager<T> {
		/// Ensure all storage items tracked by this type are valid.
		///
		/// We look into 4 mappings and their keys:
		///
		/// * All tracked assets ([`Endpoints`]).
		/// * Current prices ([`Price`]).
		/// * Historical prices ([`PriceHistory`]).
		/// * Votes ([`BlockVotes`]).
		///
		/// Note: this check should only be called at the end of a block, after `on_finalize` has
		/// been called.
		fn sanity_check() -> Result<(), sp_runtime::TryRuntimeError> {
			// 1.Tracked assets is the superset of all. An asset can be tracked, but not yet
			// have any of the latter 3 storage items.
			Self::ensure_all_assets_are_tracked()?;

			for asset_id in Self::tracked_assets() {
				if T::HistoryDepth::get() > 0 {
					// 2.1 Rounds of voting data should be equal to historical prices + 1.
					Self::ensure_asset_history_is_valid(asset_id)?;
				} else {
					// 2.2 There should be no history.
					Self::ensure_no_history(asset_id)?;
				}

				// 2.3. Ensure all votes that are in storage for this asset respect `MaxVoteAge`.
				Self::ensure_all_votes_are_valid(asset_id)?;
			}

			Ok(())
		}

		fn ensure_all_votes_are_valid(
			asset_id: T::AssetId,
		) -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				BlockVotes::<T>::iter_prefix(asset_id).all(|(target_block, votes)| {
					votes.into_iter().all(|(_who, vote)| {
						Pallet::<T>::vote_not_too_old_at(vote.produced_in, target_block)
					})
				}),
				"some vote in BlockVotes is too old"
			);
			Ok(())
		}

		fn ensure_no_history(asset_id: T::AssetId) -> Result<(), sp_runtime::TryRuntimeError> {
			// Note: we might move votes from block n to n+1 at the end of block n as a result of
			// `TallyError::KeepVotes`, these future votes don't count towards this check as an
			// exception.
			let local_block_number = Pallet::<T>::local_block_number();
			let votes_history = BlockVotes::<T>::iter_prefix(&asset_id)
				.filter(|(target_block, _vote)| target_block <= &local_block_number)
				.count();
			let price_history = PriceHistory::<T>::get(&asset_id).len();
			ensure!(
				votes_history == 0 && price_history == 0,
				"votes/price history (excluding a future block) should be empty"
			);
			Ok(())
		}

		fn ensure_asset_history_is_valid(
			asset_id: T::AssetId,
		) -> Result<(), sp_runtime::TryRuntimeError> {
			// Note: we might move votes from block n to n+1 at the end of block n as a result of
			// `TallyError::KeepVotes`, these future votes don't count towards this check as an
			// exception.
			let local_block_number = Pallet::<T>::local_block_number();
			let votes_history = BlockVotes::<T>::iter_prefix(&asset_id)
				.filter(|(target_block, _vote)| target_block <= &local_block_number)
				.count();
			let price_history = PriceHistory::<T>::get(&asset_id).len();
			ensure!(
				votes_history == 0 || votes_history == price_history + 1,
				"votes history (excluding a future block) should be equal to price history + 1"
			);
			Ok(())
		}

		fn ensure_all_assets_are_tracked() -> Result<(), sp_runtime::TryRuntimeError> {
			let tracked = Self::tracked_assets();
			let with_price = Price::<T>::iter_keys().collect::<Vec<_>>();
			let with_history = PriceHistory::<T>::iter_keys().collect::<Vec<_>>();
			let with_votes = BlockVotes::<T>::iter_keys()
				.map(|(asset_id, _block_number)| asset_id)
				.collect::<Vec<_>>();
			ensure!(
				with_price.iter().all(|x| tracked.contains(x)),
				"all assets with price should be tracked"
			);
			ensure!(
				with_history.iter().all(|x| tracked.contains(x)),
				"all assets with history should be tracked"
			);
			ensure!(
				with_votes.iter().all(|x| tracked.contains(x)),
				"all assets with votes should be tracked"
			);
			Ok(())
		}

		/// Returns all of the votes submitted associated with `block_number` for `asset_id`.
		pub(crate) fn block_votes(
			asset_id: T::AssetId,
			block_number: BlockNumberFor<T>,
		) -> Vec<(T::AccountId, Vote<BlockNumberFor<T>>)> {
			BlockVotes::<T>::get(asset_id, block_number).into_iter().collect::<Vec<_>>()
		}

		/// Return the historical price of `asset_id`, excluding the current price stored in
		/// [`Price`].
		pub(crate) fn price_history(
			asset_id: T::AssetId,
		) -> Vec<PriceData<BlockNumberFor<T>, MomentOf<T>>> {
			PriceHistory::<T>::get(asset_id).into_inner()
		}

		/// Returns a list of (block_number, vote_count) pairs for `asset_id`.
		pub(crate) fn block_with_votes(asset_id: T::AssetId) -> Vec<(BlockNumberFor<T>, u32)> {
			BlockVotes::<T>::iter_prefix(asset_id)
				.map(|(block_number, votes)| (block_number, votes.len() as u32))
				.collect::<Vec<_>>()
		}
	}

	/// The block number at which the price was updated.
	#[derive(
		TypeInfo,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Debug,
		Clone,
		Eq,
		PartialEq,
		Default,
		MaxEncodedLen,
	)]
	pub struct TimePoint<BlockNumber, Moment> {
		/// The local block number.
		pub(crate) local: BlockNumber,
		/// The relay block number.
		pub(crate) relay: BlockNumber,
		/// The canonical timestamp.
		pub(crate) timestamp: Moment,
	}

	/// A single price data-point.
	#[derive(
		TypeInfo,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Debug,
		Clone,
		Eq,
		PartialEq,
		Default,
		MaxEncodedLen,
	)]
	pub struct PriceData<BlockNumber, Moment> {
		/// The price of the asset.
		pub(crate) price: FixedU128,
		/// The confidence in the price.
		pub(crate) confidence: Percent,
		/// The time point at which the price was updated.
		pub(crate) updated_in: TimePoint<BlockNumber, Moment>,
	}

	/// A single vote data-point.
	#[derive(
		TypeInfo,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Debug,
		Clone,
		Eq,
		PartialEq,
		Default,
		MaxEncodedLen,
	)]
	pub(crate) struct Vote<BlockNumber> {
		/// The price-value of the vote.
		pub(crate) price: FixedU128,
		/// When this vote was produced in.
		pub(crate) produced_in: BlockNumber,
	}

	#[pallet::storage]
	type Endpoints<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AssetId,
		BoundedVec<BoundedVec<u8, T::MaxEndpointLength>, T::MaxEndpointsPerAsset>,
		OptionQuery,
	>;

	#[pallet::storage]
	type Price<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AssetId,
		PriceData<BlockNumberFor<T>, MomentOf<T>>,
		OptionQuery,
	>;

	/// Historical prices stored for assets.
	///
	/// Cleared automatically after [`Config::HistoryDepth`] blocks.
	#[pallet::storage]
	type PriceHistory<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AssetId,
		BoundedVec<PriceData<BlockNumberFor<T>, MomentOf<T>>, T::HistoryDepth>,
		ValueQuery,
	>;

	/// Votes submitted in in any given block.
	///
	/// This is keyed by asset-id and the LOCAL block number.
	///
	/// Cleared automatically after [`Config::HistoryDepth`] blocks.
	#[pallet::storage]
	type BlockVotes<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AssetId,
		Twox64Concat,
		BlockNumberFor<T>,
		BoundedBTreeMap<T::AccountId, Vote<BlockNumberFor<T>>, T::MaxVotesPerBlock>,
		ValueQuery,
	>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub tracked_assets: Vec<(T::AssetId, Vec<Vec<u8>>)>,
		pub maybe_authorities: Option<Vec<(T::AccountId, Percent)>>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (asset_id, endpoints) in &self.tracked_assets {
				let inner_bounded = endpoints
					.into_iter()
					.map(|e| {
						BoundedVec::<u8, T::MaxEndpointLength>::try_from(e.clone())
							.expect("genesis endpoints should fit")
					})
					.collect::<Vec<_>>();
				let outer_bounded =
					BoundedVec::<_, T::MaxEndpointsPerAsset>::try_from(inner_bounded)
						.expect("genesis endpoints should fit");
				StorageManager::<T>::register_asset(*asset_id, outer_bounded)
					.expect("failed to register genesis asset");
			}
			if let Some(authorities) = &self.maybe_authorities {
				let bounded_authorities =
					BoundedVec::<_, T::MaxAuthorities>::try_from(authorities.clone())
						.expect("genesis authorities should fit");
				Authorities::<T>::put(bounded_authorities);
			}
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(local_block_number: BlockNumberFor<T>) {
			for asset_id in StorageManager::<T>::tracked_assets() {
				Self::do_tally(asset_id, local_block_number)
			}

			#[cfg(test)]
			let _ = Self::do_try_state(local_block_number)
				.defensive_proof("try_state should not fail; qed");
		}

		fn offchain_worker(block_number: BlockNumberFor<T>) {
			let res = offchain::OracleOffchainWorker::<T>::offchain_worker(block_number);
			log!(debug, "offchain worker result: {:?}", res);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(block_number: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(block_number)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A new opinion from `origin` about the `price` of `asset_id`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::vote())]
		pub fn vote(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			price: FixedU128,
			produced_in: BlockNumberFor<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin).and_then(|who| {
				Authorities::<T>::get()
					.into_iter()
					.find_map(|(a, _c)| if a == who { Some(a) } else { None })
					.ok_or(sp_runtime::traits::BadOrigin)
			})?;

			// Ensure the call is not too old
			ensure!(Self::vote_not_too_old_now(produced_in), Error::<T>::OldVote);

			// Register it.
			StorageManager::<T>::add_vote(asset_id, who.clone(), Vote { price, produced_in })?;

			log!(
				debug,
				"vote from {:?}, asset_id: {:?}, price: {:?}, produced_in: {:?}",
				who,
				asset_id,
				price,
				produced_in
			);
			Self::deposit_event(Event::<T>::VoteSubmitted { who, asset_id, price });

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn register_asset(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			endpoints: Vec<Vec<u8>>,
		) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn deregister_asset(origin: OriginFor<T>, asset_id: T::AssetId) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn add_endpoint(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			endpoint: Vec<u8>,
		) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn remove_endpoint(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			index: u32,
		) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn force_set_authorities(origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(6)]
		#[pallet::weight(0)]
		pub fn set_invulnerables(origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(7)]
		#[pallet::weight(0)]
		pub fn ban_authority(origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}

		#[pallet::call_index(8)]
		#[pallet::weight(0)]
		pub fn unban_authority(origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}
	}

	/// Helper functions.
	impl<T: Config> Pallet<T> {
		fn do_tally(asset_id: T::AssetId, local_block_number: BlockNumberFor<T>) {
			let votes = BlockVotes::<T>::get(asset_id, local_block_number)
				.into_iter()
				.map(|(who, vote)| (who, vote.price, vote.produced_in))
				.collect::<Vec<_>>();
			match T::TallyManager::tally(asset_id, votes) {
				Ok((price, confidence)) => {
					// will store the new price, and prune old voting data as per `HistoryDepth`.
					match StorageManager::<T>::update(
						asset_id,
						price,
						confidence,
						local_block_number,
					) {
						Ok(new_price) => {
							log!(info, "updated price for asset {:?}: {:?}", asset_id, new_price);
							T::OnPriceUpdate::on_price_update(asset_id, new_price);
						},
						Err(e) => {
							defensive!("the only reason `update` might fail is if asset is not tracked; we are iterating tracked assets here; qed");
						},
					}
				},
				Err(TallyOuterError::KeepVotes(e)) => {
					// move unprocessed votes from this round to the next one, if they are not
					// stale now.
					let next_block = local_block_number + One::one();
					let mut unprocessed = BlockVotes::<T>::take(&asset_id, local_block_number);
					let original_count = unprocessed.len();
					unprocessed.retain(|k, v| Self::vote_not_too_old_at(v.produced_in, next_block));

					log!(
						error,
						"error tallying votes for asset {:?}: {:?}, keeping {} out of {} votes",
						asset_id,
						e,
						unprocessed.len(),
						original_count
					);

					BlockVotes::<T>::insert(asset_id, next_block, unprocessed);
				},
				Err(TallyOuterError::YankVotes(e)) => {
					BlockVotes::<T>::remove(asset_id, local_block_number);
					log!(
						error,
						"error tallying votes for asset {:?}: {:?}, yanking votes",
						asset_id,
						e
					);
				},
			}
		}

		/// Get the local block number.
		pub(crate) fn local_block_number() -> BlockNumberFor<T> {
			frame_system::Pallet::<T>::block_number()
		}

		/// Get the relay block number.
		pub(crate) fn relay_block_number() -> BlockNumberFor<T> {
			T::RelayBlockNumberProvider::current_block_number()
		}

		/// Determine if a vote is too old at the current block number or not.
		pub(crate) fn vote_not_too_old_now(produced_in: BlockNumberFor<T>) -> bool {
			Self::vote_not_too_old_at(produced_in, Self::local_block_number())
		}

		/// Determine if a vote is too old at a given block number or not.
		///
		/// Note: both argument are of the same type; be careful with the order.
		pub(crate) fn vote_not_too_old_at(
			produced_in: BlockNumberFor<T>,
			at: BlockNumberFor<T>,
		) -> bool {
			produced_in >= at.saturating_sub(T::MaxVoteAge::get())
		}
	}

	#[cfg(any(feature = "try-runtime", test))]
	impl<T: Config> Pallet<T> {
		pub fn do_try_state(
			block_number: BlockNumberFor<T>,
		) -> Result<(), sp_runtime::TryRuntimeError> {
			StorageManager::<T>::sanity_check()?;
			Ok(())
		}
	}

	impl<T: Config> sp_runtime::BoundToRuntimeAppPublic for Pallet<T> {
		type Public = T::AuthorityId;
	}

	impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
		type Key = T::AuthorityId;

		fn on_genesis_session<'a, I: 'a>(validators: I)
		where
			I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
		{
			let authorities =
				validators.map(|(who, _keys)| (who.clone(), Percent::one())).collect::<Vec<_>>();
			let bounded: BoundedVec<_, _> = authorities.defensive_truncate_into();
			Authorities::<T>::put(bounded);
		}

		fn on_new_session<'a, I: 'a>(changed: bool, validators: I, _queued_validators: I)
		where
			I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
		{
			if changed {
				let authorities = validators
					.map(|(who, _keys)| (who.clone(), Percent::one()))
					.collect::<Vec<_>>();
				let count = authorities.len() as u32;
				let bounded: BoundedVec<_, _> = authorities.defensive_truncate_into();
				Authorities::<T>::put(bounded);
				Self::deposit_event(Event::<T>::NewValidatorsAnnounced { count });
			}
		}

		fn on_disabled(_: u32) {
			// TODO
		}
	}
}
