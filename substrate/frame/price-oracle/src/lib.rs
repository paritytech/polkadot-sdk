#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};
use frame_support::{
	pallet_prelude::*,
	traits::{EnsureOrigin, Time},
};
use frame_system::pallet_prelude::*;
use sp_consensus_babe::AuthorityId;
use sp_consensus_slots::Slot;
use sp_price_oracle::{
	InherentError, Nudge, PairConfig, PairId, PriceOracleInherentData, SignedNudge,
	INHERENT_IDENTIFIER,
};
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128};

pub use decoders::ParsingMethod;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod decoders;
#[cfg(test)]
mod tests;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::price-oracle";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use sp_inherents::{InherentData, InherentIdentifier};

	pub trait AuthorityProvider {
		fn authorities() -> Vec<AuthorityId>;
		fn current_slot() -> Slot;
	}

	/// Called whenever an on-chain price is updated for a pair.
	/// Can be used to propagate the price via XCM to other chains (e.g. Asset Hub).
	#[impl_trait_for_tuples::impl_for_tuples(8)]
	pub trait OnPriceUpdate<BlockNumber> {
		fn on_price_update(
			pair_id: PairId,
			new_price: FixedU128,
			block_number: BlockNumber,
			timestamp: u64,
		);
	}

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		type AuthorityProvider: AuthorityProvider;

		type TimeProvider: Time;

		/// Hook called when any pair's price is updated. Set to `()` if unused.
		type OnPriceUpdate: OnPriceUpdate<BlockNumberFor<Self>>;

		/// Origin allowed to manage pairs and their endpoints.
		type PriceOracleOrigin: EnsureOrigin<Self::RuntimeOrigin>;


		/// Maximum number of endpoints per pair.
		#[pallet::constant]
		type MaxEndpoints: Get<u32>;

		/// Maximum length in bytes of a single endpoint URL.
		#[pallet::constant]
		type MaxUrlLength: Get<u32>;
	}

	/// A URL bounded by [`Config::MaxUrlLength`].
	pub type BoundedUrl<T> = BoundedVec<u8, <T as Config>::MaxUrlLength>;

	/// The per-pair endpoint list, bounded by [`Config::MaxEndpoints`] and
	/// [`Config::MaxUrlLength`].
	pub type BoundedEndpoints<T> =
		BoundedVec<(ParsingMethod, BoundedUrl<T>), <T as Config>::MaxEndpoints>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Registered asset pairs and their configuration. Pairs are added and removed via
	/// [`Pallet::register_pair`], [`Pallet::remove_pair`], and
	/// [`Pallet::update_pair_config`].
	#[pallet::storage]
	pub type Pairs<T: Config> = StorageMap<_, Blake2_128Concat, PairId, PairConfig, OptionQuery>;

	/// Current on-chain price per pair. Defaults to zero.
	#[pallet::storage]
	pub type CurrentPrice<T: Config> =
		StorageMap<_, Blake2_128Concat, PairId, FixedU128, ValueQuery>;

	/// Active endpoint list per pair, mutated via
	/// [`Pallet::set_active_endpoints`].
	#[pallet::storage]
	pub type ActiveEndpoints<T: Config> =
		StorageMap<_, Blake2_128Concat, PairId, BoundedEndpoints<T>, ValueQuery>;

	/// Whether an inherent entry was processed for the pair in the current block. Cleared
	/// during `on_finalize`.
	#[pallet::storage]
	pub type InherentSeen<T: Config> = StorageMap<_, Blake2_128Concat, PairId, bool, ValueQuery>;

	/// Re-entry guard for `submit_nudges` within a single block. Cleared during `on_finalize`.
	#[pallet::storage]
	pub(crate) type InherentCalled<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too few nudges were provided for a pair (below its per-pair minimum).
		TooFewNudges,
		/// A nudge in the inherent is too old (slot is beyond the validity window).
		StaleNudge,
		/// A nudge in the inherent has a duplicate authority within its pair.
		DuplicateNudge,
		/// A nudge in the inherent has an invalid authority.
		InvalidAuthority,
		/// A nudge in the inherent has an invalid signature.
		InvalidSignature,
		/// `submit_nudges` was called more than once in the same block.
		DuplicateInherent,
		/// Too many endpoints supplied for [`Config::MaxEndpoints`].
		TooManyEndpoints,
		/// An endpoint URL exceeded [`Config::MaxUrlLength`].
		UrlTooLong,
		/// A parsing method id does not map to a known [`ParsingMethod`].
		UnknownParsingMethod,
		/// The pair referenced is not registered on-chain.
		UnknownPair,
		/// Tried to register a pair id that is already in use.
		PairAlreadyExists,
		/// The same pair appeared more than once in a single inherent.
		DuplicatePairInInherent,

	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new pair was registered.
		PairRegistered { pair_id: PairId },
		/// A pair's config was replaced.
		PairConfigUpdated { pair_id: PairId },
		/// A pair was removed and its storage cleared.
		PairRemoved { pair_id: PairId },
		/// The endpoint list for a pair was replaced.
		ActiveEndpointsUpdated { pair_id: PairId, count: u32 },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			InherentCalled::<T>::kill();
			for (pair_id, cfg) in Pairs::<T>::iter() {
				if cfg.inherent_mandatory {
					// If the pair is mandatory, we assert that an inherent entry was included.
					assert!(
						InherentSeen::<T>::take(pair_id),
						"Price oracle: pair {} marked inherent_mandatory but no inherent entry was included",
						pair_id,
					);
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a batch of per-pair signed nudges as an inherent.
		///
		/// The outer vector groups nudges by pair. Each pair must be registered, must appear
		/// at most once, and must satisfy its per-pair config.
		///
		/// Per-pair behavior on invalid nudges is controlled by
		/// [`PairConfig::invalid_inherent_panics`]: if `true`, errors become runtime panics;
		/// if `false`, they are returned as dispatch errors and the whole block is rejected.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let p = pair_nudges.len() as u64;
			(
				Weight::from_parts(10_000, 0)
					.saturating_add(Weight::from_parts(5_000, 0).saturating_mul(p)),
				DispatchClass::Mandatory
			)
		})]
		pub fn submit_nudges(
			origin: OriginFor<T>,
			pair_nudges: Vec<(PairId, Vec<SignedNudge>)>,
		) -> DispatchResult {
			ensure_none(origin)?;
			ensure!(!InherentCalled::<T>::get(), Error::<T>::DuplicateInherent);
			InherentCalled::<T>::put(true);

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let block_number = frame_system::Pallet::<T>::block_number();
			let timestamp: u64 = T::TimeProvider::now().try_into().unwrap_or(0u64);

			let mut seen_pairs = BTreeSet::<PairId>::new();

			for (pair_id, nudges) in pair_nudges {
				ensure!(seen_pairs.insert(pair_id), Error::<T>::DuplicatePairInInherent);

				// Unknown pair always errors — we have no config to consult for the
				// "panic instead" choice.
				let cfg = Pairs::<T>::get(pair_id).ok_or(Error::<T>::UnknownPair)?;

				Self::apply_pair_nudges(
					pair_id,
					&cfg,
					&nudges,
					&authorities,
					current_slot,
					block_number,
					timestamp,
				)?;

				InherentSeen::<T>::insert(pair_id, true);
			}

			Ok(())
		}

		/// Enable or disable an existing pair's inherent-mandatory flag via a full config
		/// update — see [`Pallet::update_pair_config`]. Also available: register / remove.
		///
		/// `initial_price` seeds [`CurrentPrice`] for the new pair. Without this, the price
		/// would read as zero until the first successful inherent — and `apply_pair_nudges`
		/// only adds/subtracts `epsilon * net` from the current price, so a pair seeded with
		/// zero can never recover a meaningful starting value from nudges alone.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 3))]
		pub fn register_pair(
			origin: OriginFor<T>,
			pair_id: PairId,
			config: PairConfig,
			initial_price: FixedU128,
		) -> DispatchResult {
			T::PriceOracleOrigin::ensure_origin(origin)?;
			ensure!(!Pairs::<T>::contains_key(pair_id), Error::<T>::PairAlreadyExists);
			Pairs::<T>::insert(pair_id, config);
			CurrentPrice::<T>::insert(pair_id, initial_price);
			Self::deposit_event(Event::PairRegistered { pair_id });
			Ok(())
		}

		/// Update the config for an existing pair.
		#[pallet::call_index(2)]
		#[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
		pub fn update_pair_config(
			origin: OriginFor<T>,
			pair_id: PairId,
			config: PairConfig,
		) -> DispatchResult {
			T::PriceOracleOrigin::ensure_origin(origin)?;
			ensure!(Pairs::<T>::contains_key(pair_id), Error::<T>::UnknownPair);
			Pairs::<T>::insert(pair_id, config);
			Self::deposit_event(Event::PairConfigUpdated { pair_id });
			Ok(())
		}

		/// Remove a pair and clear all of its per-pair storage.
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(1, 5))]
		pub fn remove_pair(origin: OriginFor<T>, pair_id: PairId) -> DispatchResult {
			T::PriceOracleOrigin::ensure_origin(origin)?;
			ensure!(Pairs::<T>::contains_key(pair_id), Error::<T>::UnknownPair);
			Pairs::<T>::remove(pair_id);
			CurrentPrice::<T>::remove(pair_id);
			ActiveEndpoints::<T>::remove(pair_id);
			InherentSeen::<T>::remove(pair_id);
			Self::deposit_event(Event::PairRemoved { pair_id });
			Ok(())
		}

		/// Replace the active endpoint list for a given pair.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(endpoints.len() as u64))]
		pub fn set_active_endpoints(
			origin: OriginFor<T>,
			pair_id: PairId,
			endpoints: Vec<(u8, Vec<u8>)>,
		) -> DispatchResult {
			T::PriceOracleOrigin::ensure_origin(origin)?;
			ensure!(Pairs::<T>::contains_key(pair_id), Error::<T>::UnknownPair);
			let converted: Vec<(ParsingMethod, BoundedUrl<T>)> = endpoints
				.into_iter()
				.map(|(id, url)| {
					let method = ParsingMethod::try_from(id)
						.map_err(|_| Error::<T>::UnknownParsingMethod)?;
					let url: BoundedUrl<T> = url.try_into().map_err(|_| Error::<T>::UrlTooLong)?;
					Ok((method, url))
				})
				.collect::<Result<_, Error<T>>>()?;
			let bounded: BoundedEndpoints<T> =
				converted.try_into().map_err(|_| Error::<T>::TooManyEndpoints)?;
			let count = bounded.len() as u32;
			ActiveEndpoints::<T>::insert(pair_id, bounded);
			Self::deposit_event(Event::ActiveEndpointsUpdated { pair_id, count });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Apply a group of nudges for a single pair. Pure validation + state transition; no
		/// panic semantics here — the caller decides whether to panic or error.
		fn apply_pair_nudges(
			pair_id: PairId,
			cfg: &PairConfig,
			nudges: &[SignedNudge],
			authorities: &[AuthorityId],
			current_slot: Slot,
			block_number: BlockNumberFor<T>,
			timestamp: u64,
		) -> Result<(), Error<T>> {
			if (nudges.len() as u32) < cfg.min_nudges {
				return Err(Error::<T>::TooFewNudges);
			}

			let mut ups: u32 = 0;
			let mut downs: u32 = 0;
			let mut seen_authorities = BTreeSet::<u32>::new();

			for nudge in nudges {
				if *nudge.slot + cfg.nudge_validity <= *current_slot {
					return Err(Error::<T>::StaleNudge);
				}

				if !seen_authorities.insert(nudge.authority_index) {
					return Err(Error::<T>::DuplicateNudge);
				}

				let authority = authorities
					.get(nudge.authority_index as usize)
					.ok_or(Error::<T>::InvalidAuthority)?;

				if !nudge.verify(authority) {
					return Err(Error::<T>::InvalidSignature);
				}

				match nudge.nudge {
					Nudge::Up => ups += 1,
					Nudge::Down => downs += 1,
				}
			}

			let total_valid = ups + downs;
			if total_valid == 0 {
				return Ok(());
			}

			let net = ups.abs_diff(downs);
			let delta = cfg.epsilon.saturating_mul(FixedU128::saturating_from_integer(net));
			let current_price = CurrentPrice::<T>::get(pair_id);
			let new_price = if ups >= downs {
				current_price.saturating_add(delta)
			} else {
				current_price.saturating_sub(delta)
			};

			log::info!(
				target: LOG_TARGET,
				"Price oracle [pair {}]: {} ups, {} downs, price {} -> {}",
				pair_id, ups, downs, current_price, new_price,
			);

			CurrentPrice::<T>::insert(pair_id, new_price);
			T::OnPriceUpdate::on_price_update(pair_id, new_price, block_number, timestamp);

			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let pair_nudges = data
				.get_data::<PriceOracleInherentData>(&INHERENT_IDENTIFIER)
				.expect("Price oracle inherent data encoded correctly")?;
			Some(Call::submit_nudges { pair_nudges })
		}

		fn check_inherent(call: &Self::Call, _data: &InherentData) -> Result<(), Self::Error> {
			let pair_nudges = match call {
				Call::submit_nudges { ref pair_nudges } => pair_nudges,
				_ => return Ok(()),
			};

			let mut seen = BTreeSet::<PairId>::new();
			for (pair_id, nudges) in pair_nudges {
				if !seen.insert(*pair_id) {
					return Err(InherentError::DuplicatePairInInherent(*pair_id));
				}
				let cfg = Pairs::<T>::get(pair_id).ok_or(InherentError::UnknownPair(*pair_id))?;
				let got = nudges.len() as u32;
				if got < cfg.min_nudges {
					return Err(InherentError::TooFewNudges(*pair_id, got, cfg.min_nudges));
				}
			}

			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::submit_nudges { .. })
		}
	}

	impl<T: Config> Pallet<T> {
		/// Current on-chain price for a pair (zero if unknown / unset).
		pub fn current_price(pair_id: PairId) -> FixedU128 {
			CurrentPrice::<T>::get(pair_id)
		}

		/// The per-pair config, or `None` if the pair is not registered.
		pub fn pair_config(pair_id: PairId) -> Option<PairConfig> {
			Pairs::<T>::get(pair_id)
		}

		/// All registered pair ids.
		pub fn list_pairs() -> Vec<PairId> {
			Pairs::<T>::iter_keys().collect()
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial pairs to register: `(pair_id, config, initial_price, endpoints)`.
		/// `initial_price` seeds [`CurrentPrice`] so reads return a real value before the
		/// first inherent. Endpoints are `(parsing_method_id, url_bytes)` pairs matching
		/// [`Pallet::set_active_endpoints`].
		pub pairs: Vec<(PairId, PairConfig, FixedU128, Vec<(u8, Vec<u8>)>)>,
		#[serde(skip)]
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (pair_id, cfg, initial_price, endpoints) in &self.pairs {
				assert!(
					!Pairs::<T>::contains_key(pair_id),
					"Price oracle genesis: duplicate pair id {}",
					pair_id,
				);
				Pairs::<T>::insert(pair_id, cfg.clone());
				CurrentPrice::<T>::insert(pair_id, *initial_price);
				let converted: Vec<(ParsingMethod, BoundedUrl<T>)> = endpoints
					.iter()
					.cloned()
					.map(|(id, url)| {
						let method = ParsingMethod::try_from(id)
							.expect("Price oracle genesis: unknown parsing method");
						let url: BoundedUrl<T> =
							url.try_into().expect("Price oracle genesis: URL too long");
						(method, url)
					})
					.collect();
				let bounded: BoundedEndpoints<T> = converted
					.try_into()
					.expect("Price oracle genesis: too many endpoints for pair");
				ActiveEndpoints::<T>::insert(pair_id, bounded);
			}
		}
	}
}
