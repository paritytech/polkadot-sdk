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

//! Price-Oracle System
//!
//! Pallets:
//!
//! - Oracle: the pallet through which validators submit their price bumps. This pallet implements a
//!   `OneSessionHandler`, allowing it to receive updated about the local session pallet. This local
//!   session pallet is controlled by the next component (`Rc-client`), and pretty much mimics the
//!   relay chain validators.
//! 	- Of course, relay validators need to use their stash key once in the price-oracle parachain
//!    to:
//! 		- Set a proxy for future use
//! 		- Associate a session key with their stash key.
//! - Rc-client: pallet that receives XCMs indicating new validator sets from the RC. It also acts
//!   as two components for the local session pallet:
//!   - `ShouldEndSession`: It immediately signals the session pallet that it should end the
//!     previous session once it receives the validator set via XCM.
//!   - `SessionManager`: Once session realizes it has to rotate the session, it will call into its
//!     `SessionManager`, which is also implemented by rc-client, to which it gives the new
//!     validator keys.
//!
//! In short, the flow is as follows:
//!
//! 1. block N: `relay_new_validator_set` is received, validators are kept as `ToPlan(v)`.
//! 2. Block N+1: `should_end_session` returns `true`.
//! 3. Block N+1: Session calls its `SessionManager`, `v` is returned in `plan_new_session`
//! 4. Block N+1: `ToPlan(v)` updated to `Planned`.
//! 5. Block N+2: `should_end_session` still returns `true`, forcing tht local session to trigger a
//!    new session again.
//! 6. Block N+2: Session again calls `SessionManager`, nothing is returned in `plan_new_session`,
//!    and session pallet will enact the `v` previously received.
//!
//! This design hinges on the fact that the session pallet always does 3 calls at the same time when
//! interacting with the `SessionManager`:
//!
//! * `end_session(n)`
//! * `start_session(n+1)`
//! * `new_session(n+2)`
//!
//! Every time `new_session` receives some validator set as return value, it is only enacted on the
//! next session rotation.
//!
//! Notes/TODOs:
//! we might want to still retain a periodic session as well, allowing validators to swap keys in
//! case of emergency.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod oracle {
	use alloc::vec::Vec;
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::{TransactionLongevity, *},
		traits::{EnsureOrigin, OneSessionHandler},
		Parameter,
	};
	use frame_system::{
		offchain::{AppCrypto, CreateBare, CreateSignedTransaction, SendSignedTransaction, Signer},
		pallet_prelude::*,
	};
	use sp_runtime::{
		offchain::Duration, traits::Member, Deserialize, FixedU128, RuntimeAppPublic, Saturating,
		Serialize,
	};

	// re-export all pallet parts, needed for runtime macros to work.
	pub use pallet::*;

	#[frame_support::pallet]
	pub mod pallet {
		use super::*;

		/// The longevity of the bump transactions. This value is not used in this pallet, and
		/// should be used in the runtime level when we construct the transaction.
		pub const LONGEVITY: TransactionLongevity = 4;

		#[pallet::config]
		pub trait Config:
			frame_system::Config + CreateSignedTransaction<Call<Self>> + CreateBare<Call<Self>>
		{
			/// The key type for the session key we use to sign [`Call::bump_price`].
			type AuthorityId: AppCrypto<Self::Public, Self::Signature>
				+ RuntimeAppPublic
				+ Parameter
				+ Member;

			/// Every `PriceUpdateInterval` blocks, the offchain worker will submit a price update
			/// transaction.
			type PriceUpdateInterval: Get<BlockNumberFor<Self>>;

			/// The type of the identifier of other assets, the price of which we are tracking
			/// against DOT.
			type AssetId: Member + Parameter + MaybeSerializeDeserialize + MaxEncodedLen;

			/// The origin that can manage this pallet, and add/remove assets and endpoints.
			///
			/// Is of utmost importance, should be super secure.
			type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		}

		#[pallet::event]
		#[pallet::generate_deposit(pub(super) fn deposit_event)]
		pub enum Event<T: Config> {
			/// A new set of validators was announced.
			NewValidatorsAnnounced { count: u32 },
			/// The price was bumped.
			Bumped { who: T::AccountId, direction: Bump },
		}

		/// Current best known authorities.
		#[pallet::storage]
		#[pallet::unbounded] // TODO
		pub type Authorities<T: Config> = StorageValue<_, Vec<T::AuthorityId>, ValueQuery>;

		#[derive(
			Encode,
			Decode,
			DecodeWithMemTracking,
			Clone,
			PartialEq,
			Eq,
			Debug,
			TypeInfo,
			Serialize,
			Deserialize,
		)]
		pub struct Asset {
			pub price: FixedU128,
			pub max_bump: FixedU128,
			pub endpoints: BoundedVec<alloc::string::String, ConstU32<10>>,
		}

		/// The assets that we are tracking, and their list of endpoints.
		#[pallet::storage]
		#[pallet::unbounded] // TODO
		pub type TrackedAssets<T: Config> =
			StorageMap<_, Twox64Concat, T::AssetId, Asset, OptionQuery>;

		#[pallet::genesis_config]
		#[derive(frame_support::DefaultNoBound)]
		pub struct GenesisConfig<T: Config> {
			pub tracked_assets: Vec<(T::AssetId, Asset)>,
		}

		#[pallet::genesis_build]
		impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
			fn build(&self) {
				for (asset_id, asset) in &self.tracked_assets {
					TrackedAssets::<T>::insert(asset_id, asset.clone());
				}
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
				let authorities = validators.map(|(_, k)| k).collect::<Vec<_>>();
				Authorities::<T>::put(authorities);
			}

			fn on_new_session<'a, I: 'a>(changed: bool, validators: I, _queued_validators: I)
			where
				I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
			{
				// instant changes
				if changed {
					let authorities = validators.map(|(_, k)| k).collect::<Vec<_>>();
					let count = authorities.len() as u32;
					Authorities::<T>::put(authorities);
					Self::deposit_event(Event::<T>::NewValidatorsAnnounced { count });
				}
			}

			fn on_disabled(_: u32) {
				todo!();
			}
		}

		#[pallet::pallet]
		pub struct Pallet<T>(_);

		#[pallet::hooks]
		impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
			fn offchain_worker(block_number: BlockNumberFor<T>) {
				if block_number % T::PriceUpdateInterval::get() != Zero::zero() {
					return;
				}

				use scale_info::prelude::vec::Vec;
				log::info!(target: "runtime::price-oracle::offchain-worker", "Offchain worker starting at #{:?}", block_number);
				let keystore_accounts =
					Signer::<T, T::AuthorityId>::keystore_accounts().collect::<Vec<_>>();
				for account in keystore_accounts.iter() {
					log::info!(target: "runtime::price-oracle::offchain-worker", "Account: {:?} / {:?} / {:?}", account.id, account.public, account.index);
				}
				let signer = Signer::<T, T::AuthorityId>::all_accounts();
				if !signer.can_sign() {
					log::error!(target: "runtime::price-oracle::offchain-worker", "cannot sign!");
					return;
				}

				let random_u8 = sp_io::offchain::random_seed()[0];
				for (asset_id, asset) in TrackedAssets::<T>::iter() {
					// pick a random endpoint
					let index = random_u8 as usize % asset.endpoints.len();
					let endpoint = &asset.endpoints[index];
					match Self::fetch_price(endpoint) {
						Ok(price) => {
							let current_price = asset.price;
							let bump = if price > current_price {
								Bump::Up((price - current_price).min(asset.max_bump))
							} else {
								Bump::Down((current_price - price).min(asset.max_bump))
							};
							log::info!(target: "runtime::price-oracle::offchain-worker", "current price is {:?}, price is {:?}, bump is {:?}", current_price, price, bump);

							let call = Call::<T>::bump_price {
								asset_id,
								bump,
								produced_in: Some(block_number),
							};
							let res = signer.send_single_signed_transaction(
								keystore_accounts.first().unwrap(),
								call,
							);
							log::info!(target: "runtime::price-oracle::offchain-worker", "submitted, result is {:?}", res);
						},
						Err(e) => {
							log::error!(target: "runtime::price-oracle::offchain-worker", "Error fetching price: {:?}", e);
						},
					};
				}
			}
		}

		#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
		pub enum Bump {
			Up(FixedU128),
			Down(FixedU128),
		}

		impl Bump {
			fn value(&self) -> FixedU128 {
				match self {
					Bump::Up(value) => *value,
					Bump::Down(value) => *value,
				}
			}

			fn apply(&self, val: FixedU128) -> FixedU128 {
				match self {
					Bump::Up(value) => val.saturating_add(*value),
					Bump::Down(value) => val.saturating_sub(*value),
				}
			}
		}

		#[pallet::error]
		pub enum Error<T> {
			AssetNotFound,
			BumpTooLarge,
		}

		#[pallet::call]
		impl<T: Config> Pallet<T> {
			#[pallet::call_index(0)]
			#[pallet::weight(0)]
			pub fn bump_price(
				origin: OriginFor<T>,
				asset_id: T::AssetId,
				bump: Bump,
				produced_in: Option<BlockNumberFor<T>>,
			) -> DispatchResult {
				let who = ensure_signed(origin).and_then(|who| {
					log::info!(
						target: "runtime::price-oracle",
						"bump_price: who is {:?}, asset_id: {:?}, bump is {:?}, produced_in: {:?}, now: {:?}",
						who,
						asset_id,
						bump,
						produced_in,
						frame_system::Pallet::<T>::block_number()
					);
					// TODO: not efficient to read all to check if person is part of. Need a
					// btreeSet
					Authorities::<T>::get()
						.into_iter()
						.find_map(
							|a| if a.encode() == who.encode() { Some(who.clone()) } else { None },
						) // TODO: bit too hacky, can improve
						.ok_or(sp_runtime::traits::BadOrigin)
				})?;

				let mut asset =
					TrackedAssets::<T>::get(asset_id.clone()).ok_or(Error::<T>::AssetNotFound)?;
				ensure!(bump.value() <= asset.max_bump, Error::<T>::BumpTooLarge);
				log::info!(target: "runtime::price-oracle", "bump_price: asset.price is {:?}, bump is {:?}", asset.price, bump);
				asset.price = bump.apply(asset.price);

				TrackedAssets::<T>::insert(asset_id, asset);
				Self::deposit_event(Event::<T>::Bumped { who, direction: bump });

				Ok(())
			}

			#[pallet::call_index(1)]
			#[pallet::weight(0)]
			pub fn track_asset(
				origin: OriginFor<T>,
				asset_id: T::AssetId,
				asset: Asset,
			) -> DispatchResult {
				T::AdminOrigin::ensure_origin(origin)?;
				TrackedAssets::<T>::insert(asset_id, asset);
				Ok(())
			}

			#[pallet::call_index(2)]
			#[pallet::weight(0)]
			pub fn remote_asset(origin: OriginFor<T>, asset_id: T::AssetId) -> DispatchResult {
				T::AdminOrigin::ensure_origin(origin)?;
				TrackedAssets::<T>::remove(asset_id);
				Ok(())
			}
		}
	}

	#[derive(Debug)]
	pub(crate) enum OffchainError {
		AssetNotFound,
		TimedOut,
		HttpError(sp_runtime::offchain::http::Error),
		CoreHttpError(sp_core::offchain::HttpError),
		UnexpectedStatusCode(u16),
		ParseError(serde_json::Error),
		Other(&'static str),
	}

	impl From<&'static str> for OffchainError {
		fn from(e: &'static str) -> Self {
			OffchainError::Other(e)
		}
	}

	impl<T: Config> Pallet<T> {
		fn fetch_price(endpoint: &str) -> Result<FixedU128, OffchainError> {
			// send request with deadline.
			let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
			let request = sp_runtime::offchain::http::Request::get(&endpoint);
			let pending =
				request.deadline(deadline).send().map_err(OffchainError::CoreHttpError)?;

			// wait til response is ready or timed out.
			let response = pending
				.try_wait(deadline)
				.map_err(|_pending_request| OffchainError::TimedOut)?
				.map_err(OffchainError::HttpError)?;

			// check status code.
			if response.code != 200 {
				return Err(OffchainError::UnexpectedStatusCode(response.code));
			}

			// extract response body.
			let body = response.body().collect::<Vec<u8>>();
			Self::parse_price(body)
		}

		fn parse_price(body: Vec<u8>) -> Result<FixedU128, OffchainError> {
			log::debug!(target: "runtime::price-oracle::offchain", "body: {:?}", body);
			let v: serde_json::Value =
				serde_json::from_slice(&body).map_err(|e| OffchainError::ParseError(e))?;
			// scenario: https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD
			match v {
				serde_json::Value::Object(obj) if obj.contains_key("USD") => {
					log::debug!(target: "runtime::price-oracle::offchain", "obj: {:?}", obj);
					use alloc::string::ToString;
					let price_str =
						obj["USD"].as_number().map(|n| n.to_string()).ok_or("failed to parse")?;
					log::debug!(target: "runtime::price-oracle::offchain", "price_str: {:?}", price_str);
					let price =
						FixedU128::from_float_str(&price_str).map_err(OffchainError::Other)?;
					Ok(price)
				},
				_ => Err(OffchainError::Other("bad json")),
			}
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		#[test]
		fn parse_price_works() {
			let test_data = vec![
				(b"{\"USD\": 100.00}".to_vec(), FixedU128::from_rational(100, 1)),
				(b"{\"USD\": 100.01}".to_vec(), FixedU128::from_rational(10001, 100)),
				(b"{\"USD\": 42.01}".to_vec(), FixedU128::from_rational(4201, 100)),
				(b"{\"USD\": 0.01}".to_vec(), FixedU128::from_rational(1, 100)),
				(b"{\"USD\": .01}".to_vec(), FixedU128::from_rational(1, 100)),
			];

			todo!();
		}

		#[test]
		fn cryptocompare_work() {
			todo!();
		}
	}
}

pub mod extensions {
	use super::oracle::Call as OracleCall;
	use codec::{Decode, DecodeWithMemTracking, Encode};
	use frame_support::{
		dispatch::DispatchInfo, pallet_prelude::TransactionSource, traits::IsSubType,
		weights::Weight,
	};
	use scale_info::TypeInfo;
	use sp_runtime::{
		traits::{
			AsSystemOriginSigner, DispatchInfoOf, Dispatchable, PostDispatchInfoOf,
			TransactionExtension, ValidateResult,
		},
		transaction_validity::{TransactionPriority, TransactionValidityError, ValidTransaction},
		DispatchResult, SaturatedConversion,
	};

	/// Transaction extension that extracts the `produced_in` field from the call body
	/// and sets it as the transaction priority.
	#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct SetPriorityFromProducedIn<T: super::oracle::Config>(core::marker::PhantomData<T>);

	impl<T: super::oracle::Config> Default for SetPriorityFromProducedIn<T> {
		fn default() -> Self {
			Self(core::marker::PhantomData)
		}
	}

	impl<T: super::oracle::Config> core::fmt::Debug for SetPriorityFromProducedIn<T> {
		fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
			write!(f, "SetPriorityFromProducedIn")
		}
	}

	impl<T> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
		for SetPriorityFromProducedIn<T>
	where
		T: super::oracle::Config + frame_system::Config + Send + Sync,
		<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
		<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin:
			AsSystemOriginSigner<T::AccountId> + Clone,
		<T as frame_system::Config>::RuntimeCall: IsSubType<OracleCall<T>>,
	{
		const IDENTIFIER: &'static str = "SetPriorityFromProducedIn";
		type Implicit = ();
		type Val = ();
		type Pre = ();

		fn weight(&self, _call: &<T as frame_system::Config>::RuntimeCall) -> Weight {
			// Minimal weight as this is just reading from the call
			Weight::from_parts(1_000, 0)
		}

		fn validate(
			&self,
			origin: <T as frame_system::Config>::RuntimeOrigin,
			call: &<T as frame_system::Config>::RuntimeCall,
			_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
			_len: usize,
			_self_implicit: Self::Implicit,
			_inherited_implication: &impl Encode,
			_source: TransactionSource,
		) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
			let mut priority: TransactionPriority = 0;

			// Check if our call `IsSubType` of the `RuntimeCall`
			if let Some(OracleCall::bump_price { produced_in, .. }) = call.is_sub_type() {
				if let Some(block_number) = produced_in {
					// Use the block number as priority
					priority = (*block_number).saturated_into();
				}
			} else {
				log::warn!(target: "runtime::price-oracle::priority-extension", "Unknown call, not setting priority")
			}

			let validity = ValidTransaction { priority, ..Default::default() };

			Ok((validity, (), origin))
		}

		fn prepare(
			self,
			_val: Self::Val,
			_origin: &<T as frame_system::Config>::RuntimeOrigin,
			_call: &<T as frame_system::Config>::RuntimeCall,
			_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
			_len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			Ok(())
		}

		fn post_dispatch_details(
			_pre: Self::Pre,
			_info: &DispatchInfo,
			_post_info: &PostDispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
			_len: usize,
			_result: &DispatchResult,
		) -> Result<Weight, TransactionValidityError> {
			Ok(Weight::zero())
		}
	}
}

pub mod rc_client {
	pub use pallet::*;

	#[frame_support::pallet]
	pub mod pallet {
		use frame_support::pallet_prelude::*;
		use frame_system::pallet_prelude::{BlockNumberFor, *};
		extern crate alloc;
		use alloc::vec::Vec;

		#[pallet::config]
		pub trait Config: frame_system::Config {
			type RelayChainOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		}

		#[pallet::pallet]
		pub struct Pallet<T>(_);

		#[derive(
			Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, Default,
		)]
		pub enum ValidatorSet<AccountId> {
			/// We don't have a validator set yet.
			#[default]
			None,
			/// We have a validator set, but we have not given it to the session pallet to be
			/// planned yet.
			ToPlan(Vec<AccountId>),
			/// A validator set was just given to the session pallet to be planned.
			///
			/// We should immediately signal the session pallet to trigger a new session, and
			/// activate it.
			Planned,
		}

		impl<AccountId> ValidatorSet<AccountId> {
			fn should_end_session(&self) -> bool {
				matches!(self, ValidatorSet::ToPlan(_) | ValidatorSet::Planned)
			}

			fn new_session(self) -> (Self, Option<Vec<AccountId>>) {
				match self {
					Self::None => {
						debug_assert!(false, "we should never instruct session to trigger a new session if we have no validator set to plan");
						(Self::None, None)
					},
					// We have something to be planned, return it, and set our next stage to
					// `planned`.
					Self::ToPlan(to_plan) => (Self::Planned, Some(to_plan)),
					// We just planned something, don't plan return anything new to be planned,
					// just let session enact what was previously planned. Set our next stage to
					// `None`.
					Self::Planned => (Self::None, None),
				}
			}
		}

		#[pallet::storage]
		#[pallet::unbounded]
		pub type ValidatorSetStorage<T: Config> =
			StorageValue<_, ValidatorSet<T::AccountId>, ValueQuery>;

		#[pallet::call]
		impl<T: Config> Pallet<T> {
			#[pallet::call_index(0)]
			#[pallet::weight(0)]
			pub fn relay_new_validator_set(
				origin: OriginFor<T>,
				validators: Vec<T::AccountId>,
			) -> DispatchResult {
				log::info!(target: "runtime::price-oracle", "relay_new_validator_set: validators: {:?}", validators);
				T::RelayChainOrigin::ensure_origin_or_root(origin)?;
				ValidatorSetStorage::<T>::put(ValidatorSet::ToPlan(validators));
				Ok(())
			}
		}

		impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
			fn should_end_session(_now: BlockNumberFor<T>) -> bool {
				log::info!(target: "runtime::price-oracle", "should_end_session: {:?}", ValidatorSetStorage::<T>::get().should_end_session());
				ValidatorSetStorage::<T>::get().should_end_session()
			}
		}

		impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
			fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
				log::info!(target: "runtime::price-oracle", "new_session: {:?}", new_index);
				let (next, ret) = ValidatorSetStorage::<T>::get().new_session();
				ValidatorSetStorage::<T>::put(next);
				ret
			}
			fn end_session(_end_index: u32) {
				// nada
			}
			fn start_session(_start_index: u32) {
				// nada
			}
		}
	}
}
