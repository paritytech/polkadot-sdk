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

#[cfg(test)]
mod mock;
pub mod pair;
pub mod pricing;
pub mod registry;
pub mod schema;
pub mod signers;
pub mod venues;
pub mod weights;

use alloc::{collections::BTreeMap, vec::Vec};
use frame_support::{
	pallet_prelude::*,
	traits::{PricePoint, PriceProvider},
};
use frame_system::pallet_prelude::*;
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError};
use sp_price_oracle::{
	inherents::{PriceOracleInherentDataExt, INHERENT_IDENTIFIER},
	market::{Market, MarketId, QueryTag, VenueId},
	runtime_api::ParseError,
	Anchor, PairId, Price, Quote, SignedPriceReport,
};
use sp_runtime::{
	traits::{BlockNumberProvider, CheckedMul, SaturatedConversion, Zero},
	RuntimeAppPublic,
};

const LOG_TARGET: &str = "runtime::price-oracle";

/// A signed report as accepted by the pallet.
pub type SignedPriceReportOf<T> =
	SignedPriceReport<<T as Config>::SignerId, <T as Config>::SignerSignature>;

pub use pair::Pairs;
pub use pallet::*;
pub use pricing::PairSettings;
pub use registry::{StoredMarket, Venue};
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

		/// The pairs the runtime knows and how they derive from each other.
		type Pairs: Pairs;

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

	/// Health limits per pair. A pair without settings is not priced.
	#[pallet::storage]
	pub type Settings<T: Config> = StorageMap<_, Twox64Concat, PairId, PairSettings, OptionQuery>;

	/// Registered venues.
	#[pallet::storage]
	pub type Venues<T: Config> = StorageMap<_, Twox64Concat, VenueId, Venue, OptionQuery>;

	/// Registered markets.
	#[pallet::storage]
	pub type Markets<T: Config> = StorageMap<_, Twox64Concat, MarketId, StoredMarket, OptionQuery>;

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
		/// A venue was added or updated.
		VenueSet { id: VenueId },
		/// A venue was removed.
		VenueRemoved { id: VenueId },
		/// A market was added or updated.
		MarketSet { id: MarketId },
		/// A market was removed.
		MarketRemoved { id: MarketId },
		/// The health limits of a pair were set.
		PairSettingsSet { pair: PairId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A parameter is out of its valid range.
		InvalidParameters,
		/// The venue is not registered.
		UnknownVenue,
		/// The market is not registered.
		UnknownMarket,
		/// The pair is not known to the runtime.
		UnknownPair,
		/// The venue still has markets.
		VenueInUse,
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

		/// Add or update a venue.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::set_venue())]
		pub fn set_venue(origin: OriginFor<T>, id: VenueId, venue: Venue) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Venues::<T>::insert(id, venue);
			Self::deposit_event(Event::VenueSet { id });
			Ok(())
		}

		/// Remove a venue. Fails while any market refers to it.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::remove_venue())]
		pub fn remove_venue(origin: OriginFor<T>, id: VenueId) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(Venues::<T>::contains_key(id), Error::<T>::UnknownVenue);
			ensure!(!Markets::<T>::iter_values().any(|m| m.venue == id), Error::<T>::VenueInUse);
			Venues::<T>::remove(id);
			Self::deposit_event(Event::VenueRemoved { id });
			Ok(())
		}

		/// Add or update a market. Its venue must be registered and its pair known.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_market())]
		pub fn set_market(
			origin: OriginFor<T>,
			id: MarketId,
			market: StoredMarket,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(Venues::<T>::contains_key(market.venue), Error::<T>::UnknownVenue);
			ensure!(T::Pairs::is_known(market.pair), Error::<T>::UnknownPair);
			ensure!(!market.contract_size.is_zero(), Error::<T>::InvalidParameters);
			Markets::<T>::insert(id, market);
			Self::deposit_event(Event::MarketSet { id });
			Ok(())
		}

		/// Set the health limits of a pair.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_pair_settings())]
		pub fn set_pair_settings(
			origin: OriginFor<T>,
			pair: PairId,
			settings: PairSettings,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(T::Pairs::is_known(pair), Error::<T>::UnknownPair);
			ensure!(!settings.impact_size.is_zero(), Error::<T>::InvalidParameters);
			Settings::<T>::insert(pair, settings);
			Self::deposit_event(Event::PairSettingsSet { pair });
			Ok(())
		}

		/// Remove a market.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::remove_market())]
		pub fn remove_market(origin: OriginFor<T>, id: MarketId) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(Markets::<T>::contains_key(id), Error::<T>::UnknownMarket);
			Markets::<T>::remove(id);
			Self::deposit_event(Event::MarketRemoved { id });
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

impl<T: Config> Pallet<T> {
	/// The active markets in wire form, as served to the oracle nodes.
	///
	/// Backs [`PriceOracleMarketApi::markets`](sp_price_oracle::runtime_api::PriceOracleMarketApi::markets).
	pub fn active_markets() -> Vec<Market> {
		Markets::<T>::iter()
			.filter(|(_, m)| m.active)
			.map(|(id, m)| m.to_wire(id))
			.collect()
	}

	/// Price one market from the responses to its queries, at the node's time `now_ms`.
	///
	/// See [`price_market`].
	///
	/// Backs [`PriceOracleMarketApi::parse`](sp_price_oracle::runtime_api::PriceOracleMarketApi::parse).
	pub fn parse_market(
		id: MarketId,
		responses: Vec<(QueryTag, Vec<u8>)>,
		now_ms: u64,
	) -> Result<Price, ParseError> {
		let err = |s: &str| ParseError(s.as_bytes().to_vec());
		let market = Markets::<T>::get(id).ok_or_else(|| err("unknown market"))?;
		let settings =
			Settings::<T>::get(market.pair).ok_or_else(|| err("pair has no settings"))?;
		price_market(&market, &settings, responses, now_ms)
	}

	/// Anchor of the latest vote on chain, per signer. Lets block authors skip reports that are
	/// already on chain.
	///
	/// Backs [`PriceOracleApi::latest_anchors`](sp_price_oracle::runtime_api::PriceOracleApi::latest_anchors).
	pub fn latest_anchors() -> Vec<(T::SignerId, Anchor)> {
		let mut latest: BTreeMap<T::SignerId, Anchor> = BTreeMap::new();
		for vote in Votes::<T>::iter_values().flatten() {
			latest
				.entry(vote.signer)
				.and_modify(|a| *a = (*a).max(vote.anchor))
				.or_insert(vote.anchor);
		}
		latest.into_iter().collect()
	}

	/// Aggregate market prices into the pair prices a node reports.
	///
	/// Backs [`PriceOracleMarketApi::aggregate`](sp_price_oracle::runtime_api::PriceOracleMarketApi::aggregate).
	pub fn aggregate_markets(prices: Vec<(MarketId, Price)>) -> Vec<Quote> {
		let prices = prices
			.into_iter()
			.filter_map(|(id, price)| {
				let m = Markets::<T>::get(id)?;
				Some((m.venue, m.pair, price))
			})
			.collect();
		pricing::aggregate(prices, &T::Pairs::all(), T::Pairs::conversions)
	}
}

/// Price `market` from the responses to its queries, at the node's time `now_ms`.
///
/// Needs one order book and one trades response among the market's queries. A response larger
/// than its query allows is rejected.
pub fn price_market(
	market: &StoredMarket,
	settings: &PairSettings,
	responses: Vec<(QueryTag, Vec<u8>)>,
	now_ms: u64,
) -> Result<Price, ParseError> {
	use schema::Parsed;
	let err = |s: &str| ParseError(s.as_bytes().to_vec());

	let mut book = None;
	let mut latest_trade_ms = None;
	for (tag, body) in responses {
		let Some(query) = market.queries.iter().find(|q| q.tag == tag) else {
			return Err(err("unknown query tag"));
		};
		if body.len() > query.request.max_response_bytes as usize {
			return Err(err("response too large"));
		}
		match query.schema.read(&body) {
			Ok(Parsed::OrderBook(mut b)) => {
				// Bring amounts quoted in contracts to base asset units.
				for level in b.bids.iter_mut().chain(b.asks.iter_mut()) {
					level.amount = level
						.amount
						.checked_mul(&market.contract_size)
						.ok_or_else(|| err("amount overflow"))?;
				}
				book = Some(b);
			},
			Ok(Parsed::LatestTradeMs(t)) => latest_trade_ms = Some(t),
			Err(e) => return Err(ParseError(alloc::format!("{e:?}").into_bytes())),
		}
	}
	let book = book.ok_or_else(|| err("no order book"))?;
	let latest_trade_ms = latest_trade_ms.ok_or_else(|| err("no trades"))?;

	pricing::market_price(&book, latest_trade_ms, now_ms, settings)
		.map_err(|e| ParseError(alloc::format!("{e:?}").into_bytes()))
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
		let mut latest: BTreeMap<T::SignerId, SignedPriceReportOf<T>> = BTreeMap::new();

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
			match latest.get(&report.signer) {
				Some(existing) if existing.report.anchor >= report.report.anchor => continue,
				_ => {
					latest.insert(report.signer.clone(), report);
				},
			}
		}

		latest.into_values().collect()
	}

	/// Merge the votes of `reports` into storage, drop expired votes, and recompute prices.
	fn apply_votes(reports: Vec<SignedPriceReportOf<T>>, current: Anchor, params: &Parameters) {
		let mut votes: BTreeMap<PairId, BoundedVec<Vote<T::SignerId>, T::MaxSigners>> =
			Votes::<T>::iter().collect();

		for report in reports {
			let anchor = report.report.anchor;
			for quote in report.report.quotes {
				if !T::Pairs::is_known(quote.pair) {
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
