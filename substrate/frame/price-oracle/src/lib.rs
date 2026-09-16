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

//! # Price Oracle Pallet
//!
//! Aggregates signed price reports of the oracle nodes of the network into on-chain prices.
//!
//! Oracle nodes fetch the markets registered in this pallet, price them through the pallet's
//! runtime APIs, and sign the resulting pair prices as a report. Block authors include the
//! reports they collected as an inherent. The pallet verifies the reports against the accepted
//! signer set, keeps the latest report of every signer, and publishes the median price of every
//! pair once enough signers have reported it.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod pricing;
pub mod signers;
pub mod weights;

use alloc::{collections::BTreeMap, vec::Vec};
use frame_support::{
	pallet_prelude::*,
	traits::{Contains, PricePoint, PriceProvider},
};
use frame_system::pallet_prelude::*;
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError};
use sp_price_oracle::{
	inherents::{PriceOracleInherentDataExt, INHERENT_IDENTIFIER},
	Anchor, PairId, Price, SignedPriceReport,
};
use sp_runtime::{
	traits::{BlockNumberProvider, SaturatedConversion},
	RuntimeAppPublic,
};

const LOG_TARGET: &str = "runtime::price-oracle";

/// A signed report as accepted by the pallet.
pub type SignedPriceReportOf<T> =
	SignedPriceReport<<T as Config>::SignerId, <T as Config>::SignerSignature>;

pub use pallet::*;
pub use signers::Signers;
pub use weights::WeightInfo;

/// Block number of [`Config::BlockNumberProvider`].
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// The latest price one signer reported for a pair.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Vote<Id> {
	/// The signer.
	pub signer: Id,
	/// Anchor of the report the price was taken from.
	///
	/// e.g. block or slot number.
	pub anchor: Anchor,
	/// The reported price.
	pub price: Price,
}

/// Tunable parameters of the pallet.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Parameters {
	/// Reports anchored more than this many blocks ago are ignored.
	pub report_window: u32,
	/// Minimum number of signers voting on a pair for it to have a price.
	pub quorum: u32,
	/// Time between two ticks of an oracle node, in milliseconds.
	pub tick_interval_ms: u32,
}

/// Provides the current anchor, against which the age of reports is measured.
///
/// Oracle nodes anchor their reports to the same clock, so it must be one both the runtime and
/// the nodes can observe.
///
/// TODO: a block number of this chain stops advancing while the chain stalls, so reports made
/// before a stall look fresh after it. Reconsider once the node side exists, e.g. relay slots.
pub trait AnchorProvider {
	/// The current anchor.
	fn current_anchor() -> Anchor;
}

impl<P: BlockNumberProvider> AnchorProvider for P {
	fn current_anchor() -> Anchor {
		Anchor(P::current_block_number().saturated_into())
	}
}

/// Which parts of the pallet are paused.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct Pause {
	/// Reports are ignored. Votes and prices do not change.
	pub processing: bool,
	/// Prices are neither exposed to consumers nor announced through [`OnPriceUpdate`].
	pub publishing: bool,
}

/// Called when the on-chain price of a pair changes.
#[impl_trait_for_tuples::impl_for_tuples(8)]
pub trait OnPriceUpdate {
	/// `price` is the new price of `pair`.
	fn on_price_update(pair: sp_price_oracle::PairId, price: sp_price_oracle::Price);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The key type oracle nodes sign reports with.
		type SignerId: Member
			+ Parameter
			+ Ord
			+ RuntimeAppPublic<Signature = Self::SignerSignature>
			+ MaxEncodedLen;

		/// The signature type of [`Config::SignerId`].
		type SignerSignature: Member + Parameter;

		/// Supplies the set of keys whose reports are accepted.
		type Signers: Signers<Self::SignerId>;

		/// Upper bound on the size of the signer set.
		#[pallet::constant]
		type MaxSigners: Get<u32>;

		/// The pairs the runtime knows. Votes on any other pair are ignored.
		type KnownPairs: Contains<PairId>;

		/// Provides the anchor reports are measured against.
		type AnchorProvider: AnchorProvider;

		/// Provides the block number stored with every price, marking its age.
		///
		/// Should advance at a steady pace in wall clock time, so that consumers can judge how
		/// old a price is.
		type BlockNumberProvider: BlockNumberProvider;

		/// Origin allowed to manage venues, markets, schemas and parameters.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Called when the price of a pair changes.
		type OnPriceUpdate: OnPriceUpdate;

		/// Weights of the pallet's calls and hooks.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Latest vote of every signer, per pair. Sorted by signer.
	#[pallet::storage]
	pub type Votes<T: Config> = StorageMap<
		_,
		Twox64Concat,
		PairId,
		BoundedVec<Vote<T::SignerId>, T::MaxSigners>,
		ValueQuery,
	>;

	/// Current price per pair, stamped with the block number of [`Config::BlockNumberProvider`].
	#[pallet::storage]
	pub type Prices<T: Config> =
		StorageMap<_, Twox64Concat, PairId, PricePoint<ProvidedBlockNumberOf<T>>, OptionQuery>;

	/// Tunable parameters, set by [`Config::AdminOrigin`].
	///
	/// While unset, no reports are accepted and no prices are published.
	#[pallet::storage]
	pub type Params<T: Config> = StorageValue<_, Parameters, OptionQuery>;

	/// Which parts of the pallet are paused, set by [`Config::AdminOrigin`].
	#[pallet::storage]
	pub type Paused<T: Config> = StorageValue<_, Pause, ValueQuery>;

	/// Whether the inherent has been processed in the current block.
	#[pallet::storage]
	pub(super) type DidProcess<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The price of a pair changed.
		PriceUpdated { pair: PairId, price: Price, signers: u32 },
		/// The reports of a block were processed.
		ReportsProcessed { accepted: u32, rejected: u32 },
		/// The parameters were set.
		ParametersSet { params: Parameters },
		/// The pause flags were set.
		PauseSet { pause: Pause },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A parameter is out of its valid range.
		InvalidParameters,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			DidProcess::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Process the price reports collected by the block author.
		///
		/// Reports that are stale, from an unknown signer, or carry an invalid signature are
		/// ignored. The votes of the remaining reports replace older votes of the same signers,
		/// expired votes are dropped, and the price of every pair with enough votes is
		/// recomputed.
		#[pallet::call_index(0)]
		#[pallet::weight((
			T::WeightInfo::process_reports(reports.len() as u32),
			DispatchClass::Mandatory
		))]
		pub fn process_reports(
			origin: OriginFor<T>,
			reports: Vec<SignedPriceReportOf<T>>,
		) -> DispatchResult {
			ensure_none(origin)?;
			assert!(
				!DidProcess::<T>::exists(),
				"Price reports must be processed only once in the block"
			);
			DidProcess::<T>::put(true);

			let Some(params) = Params::<T>::get() else {
				log::debug!(target: LOG_TARGET, "Parameters unset, ignoring {} reports", reports.len());
				return Ok(());
			};
			if Paused::<T>::get().processing {
				log::debug!(target: LOG_TARGET, "Processing paused, ignoring {} reports", reports.len());
				return Ok(());
			}
			let anchor = T::AnchorProvider::current_anchor();
			let total = reports.len() as u32;

			let accepted = Self::filter_reports(reports, anchor, params.report_window);
			Self::deposit_event(Event::ReportsProcessed {
				accepted: accepted.len() as u32,
				rejected: total - accepted.len() as u32,
			});

			Self::apply_votes(accepted, anchor, &params);
			Ok(())
		}

		/// Set the parameters. Reports are processed once they are set.
		///
		/// `report_window` and `quorum` must be at least one.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_parameters())]
		pub fn set_parameters(origin: OriginFor<T>, params: Parameters) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(params.report_window >= 1 && params.quorum >= 1, Error::<T>::InvalidParameters);
			Params::<T>::put(&params);
			Self::deposit_event(Event::ParametersSet { params });
			Ok(())
		}

		/// Set which parts of the pallet are paused.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_pause())]
		pub fn set_pause(origin: OriginFor<T>, pause: Pause) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Paused::<T>::put(pause);
			Self::deposit_event(Event::PauseSet { pause });
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let reports =
				PriceOracleInherentDataExt::<T::SignerId, T::SignerSignature>::price_reports(data)
					.unwrap_or_else(|e| {
						log::warn!(target: LOG_TARGET, "Malformed price oracle inherent data: {e:?}");
						None
					})
					.unwrap_or_default();
			Some(Call::process_reports { reports })
		}

		fn check_inherent(_call: &Self::Call, _data: &InherentData) -> Result<(), Self::Error> {
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::process_reports { .. })
		}
	}
}

impl<T: Config> PriceProvider for Pallet<T> {
	type Pair = PairId;
	type BlockNumber = ProvidedBlockNumberOf<T>;

	/// `None` while publishing is paused.
	fn price(pair: PairId) -> Option<PricePoint<Self::BlockNumber>> {
		if Paused::<T>::get().publishing {
			return None;
		}
		Prices::<T>::get(pair)
	}
}

/// A vote's anchor is live if it is not ahead of the current anchor and within the report window.
fn is_live(anchor: Anchor, current: Anchor, window: u32) -> bool {
	anchor <= current && current.0 - anchor.0 <= window
}

impl<T: Config> Pallet<T> {
	/// Drop reports that are stale, from an unknown signer, wrongly signed, or superseded by a
	/// newer report of the same signer in the same batch.
	fn filter_reports(
		reports: Vec<SignedPriceReportOf<T>>,
		current: Anchor,
		window: u32,
	) -> Vec<SignedPriceReportOf<T>> {
		let signers = T::Signers::signers();
		let mut newest: BTreeMap<T::SignerId, SignedPriceReportOf<T>> = BTreeMap::new();

		for report in reports {
			if !is_live(report.report.anchor, current, window) {
				log::debug!(target: LOG_TARGET, "Stale report anchored at {:?}, current anchor {current:?}", report.report.anchor);
				continue;
			}
			if !signers.contains(&report.signer) {
				log::debug!(target: LOG_TARGET, "Report from unknown signer {:?}", report.signer);
				continue;
			}
			if !report.verify_signature() {
				log::debug!(target: LOG_TARGET, "Invalid signature from {:?}", report.signer);
				continue;
			}
			match newest.get(&report.signer) {
				Some(existing) if existing.report.anchor >= report.report.anchor => continue,
				_ => {
					newest.insert(report.signer.clone(), report);
				},
			}
		}

		newest.into_values().collect()
	}

	/// Merge the votes of `reports` into storage, drop expired votes, and recompute prices.
	fn apply_votes(reports: Vec<SignedPriceReportOf<T>>, current: Anchor, params: &Parameters) {
		let mut votes: BTreeMap<PairId, BoundedVec<Vote<T::SignerId>, T::MaxSigners>> =
			Votes::<T>::iter().collect();

		for report in reports {
			let anchor = report.report.anchor;
			for quote in report.report.quotes {
				if !T::KnownPairs::contains(&quote.pair) {
					continue;
				}
				let pair_votes = votes.entry(quote.pair).or_default();
				let vote = Vote { signer: report.signer.clone(), anchor, price: quote.price };
				match pair_votes.binary_search_by(|v| v.signer.cmp(&vote.signer)) {
					Ok(i) if pair_votes[i].anchor < anchor => pair_votes[i] = vote,
					Ok(_) => {},
					Err(i) => {
						if pair_votes.try_insert(i, vote).is_err() {
							log::warn!(target: LOG_TARGET, "Vote storage of {:?} is full", quote.pair);
						}
					},
				}
			}
		}

		let publishing_paused = Paused::<T>::get().publishing;
		let updated_at = T::BlockNumberProvider::current_block_number();

		for (pair, pair_votes) in votes.iter_mut() {
			pair_votes.retain(|v| is_live(v.anchor, current, params.report_window));
			if pair_votes.is_empty() || (pair_votes.len() as u32) < params.quorum {
				continue;
			}
			let mut prices: Vec<Price> = pair_votes.iter().map(|v| v.price).collect();
			let Some(price) = pricing::median(&mut prices) else { continue };
			let signers = pair_votes.len() as u32;

			let changed = Prices::<T>::get(pair).map_or(true, |p| p.price != price);
			Prices::<T>::insert(pair, PricePoint { price, updated_at, signers });
			if changed {
				Self::deposit_event(Event::PriceUpdated { pair: *pair, price, signers });
				if !publishing_paused {
					T::OnPriceUpdate::on_price_update(*pair, price);
				}
			}
		}

		for (pair, pair_votes) in votes {
			if pair_votes.is_empty() {
				Votes::<T>::remove(pair);
			} else {
				Votes::<T>::insert(pair, pair_votes);
			}
		}
	}
}
