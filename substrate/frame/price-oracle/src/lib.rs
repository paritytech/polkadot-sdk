#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use frame_support::traits::Time;
use frame_system::pallet_prelude::*;
use sp_consensus_babe::AuthorityId;
use sp_consensus_slots::Slot;
use sp_price_oracle::{
	InherentError, Nudge, PriceOracleInherentData, SignedNudge, INHERENT_IDENTIFIER,
};
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128};

pub use decoders::ParsingMethod;

pub mod decoders;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
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

	/// Called whenever the on-chain price is updated.
	/// Can be used to propagate the price via XCM to other chains (e.g. Asset Hub).
	#[impl_trait_for_tuples::impl_for_tuples(8)]
	pub trait OnPriceUpdate<BlockNumber> {
		fn on_price_update(new_price: FixedU128, block_number: BlockNumber, timestamp: u64);
	}

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Absolute price change per net nudge (e.g. 0.001 means each net Up adds $0.001).
		#[pallet::constant]
		type Epsilon: Get<FixedU128>;

		/// Minimum valid nudges required per block. Block panics in `on_finalize` if not met.
		/// Set to 0 to make oracle inherents optional.
		#[pallet::constant]
		type MinNudges: Get<u32>;

		/// Number of slots a nudge remains valid: [slot, slot + NudgeValidity).
		#[pallet::constant]
		type NudgeValidity: Get<u64>;

		type AuthorityProvider: AuthorityProvider;

		type TimeProvider: Time;

		/// Hook called when the price is updated. Set to `()` if unused.
		type OnPriceUpdate: OnPriceUpdate<BlockNumberFor<Self>>;

		/// Maximum number of entries allowed in [`ActiveEndpoints`].
		#[pallet::constant]
		type MaxEndpoints: Get<u32>;

		/// Maximum length in bytes of a single endpoint URL in [`ActiveEndpoints`].
		#[pallet::constant]
		type MaxUrlLength: Get<u32>;
	}

	/// A URL bounded by [`Config::MaxUrlLength`].
	pub type BoundedUrl<T> = BoundedVec<u8, <T as Config>::MaxUrlLength>;

	/// The active endpoint list, bounded by [`Config::MaxEndpoints`] and
	/// [`Config::MaxUrlLength`].
	pub type BoundedEndpoints<T> =
		BoundedVec<(ParsingMethod, BoundedUrl<T>), <T as Config>::MaxEndpoints>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type CurrentPrice<T: Config> = StorageValue<_, FixedU128, ValueQuery>;

	/// Number of valid nudges accepted in the current block's inherent.
	#[pallet::storage]
	pub(crate) type NudgeCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The set of price feed endpoints currently queried by the node.
	/// Each entry is a `(ParsingMethod, url_bytes)` pair. Mutated via the
	/// root-only [`Pallet::set_active_endpoints`] extrinsic.
	#[pallet::storage]
	pub type ActiveEndpoints<T: Config> = StorageValue<_, BoundedEndpoints<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The active endpoint list was replaced.
		ActiveEndpointsUpdated { count: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Too many endpoints supplied for [`Config::MaxEndpoints`].
		TooManyEndpoints,
		/// An endpoint URL exceeded [`Config::MaxUrlLength`].
		UrlTooLong,
		/// A parsing method id does not map to a known [`ParsingMethod`].
		UnknownParsingMethod,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			let count = NudgeCount::<T>::take();
			let min = T::MinNudges::get();
			if min > 0 {
				assert!(
					count >= min,
					"Price oracle: got {} valid nudges, need at least {}",
					count,
					min,
				);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a set of signed nudges as an inherent.
		///
		/// Validation is intentionally graceful: invalid nudges (stale, duplicate authority,
		/// bad signature, unknown authority) are skipped rather than causing a block rejection.
		/// This is because:
		/// - The slot visible to the block author's node may differ by 1 from the runtime's slot
		///   (node reads parent header, runtime has the current block's slot), making borderline
		///   nudges valid on one side but stale on the other.
		/// - The validator set can change between when a nudge was signed and when it's included,
		///   making a previously-valid authority index invalid.
		///
		/// We may change any of them to a panic if needed.
		///
		/// `check_inherent` (run by importers) only enforces `MinNudges` as a reasonableness
		/// check. All per-nudge validation happens here.
		#[pallet::call_index(0)]
		#[pallet::weight((
			Weight::from_parts(10_000, 0).saturating_mul(nudges.len() as u64),
			DispatchClass::Mandatory
		))]
		pub fn submit_nudges(origin: OriginFor<T>, nudges: Vec<SignedNudge>) -> DispatchResult {
			ensure_none(origin)?;
			assert!(
				!NudgeCount::<T>::exists(),
				"Price oracle inherent must be submitted only once per block"
			);

			let authorities = T::AuthorityProvider::authorities();
			let current_slot = T::AuthorityProvider::current_slot();
			let validity = T::NudgeValidity::get();
			let epsilon = T::Epsilon::get();

			let mut ups: u32 = 0;
			let mut downs: u32 = 0;
			let mut seen_authorities = alloc::collections::BTreeSet::<u32>::new();

			for nudge in &nudges {
				if *nudge.slot + validity <= *current_slot {
					log::warn!(
						target: LOG_TARGET,
						"Stale nudge from slot {:?}, current slot {:?}, validity {}",
						nudge.slot, current_slot, validity,
					);
					continue;
				}

				if !seen_authorities.insert(nudge.authority_index) {
					log::warn!(
						target: LOG_TARGET,
						"Duplicate nudge from authority index {}, skipping",
						nudge.authority_index,
					);
					continue;
				}

				let authority = match authorities.get(nudge.authority_index as usize) {
					Some(a) => a,
					None => {
						log::warn!(
							target: LOG_TARGET,
							"Invalid authority index {}",
							nudge.authority_index,
						);
						continue;
					},
				};

				if !nudge.verify(authority) {
					log::warn!(
						target: LOG_TARGET,
						"Invalid signature for authority index {}",
						nudge.authority_index,
					);
					continue;
				}

				match nudge.nudge {
					Nudge::Up => ups += 1,
					Nudge::Down => downs += 1,
				}
			}

			let total_valid = ups + downs;
			let current_price = CurrentPrice::<T>::get();
			if total_valid > 0 {
				let net = if ups >= downs { ups - downs } else { downs - ups };
				let delta = epsilon.saturating_mul(FixedU128::saturating_from_integer(net));

				let new_price = if ups >= downs {
					current_price.saturating_add(delta)
				} else {
					current_price.saturating_sub(delta)
				};

				log::info!(
					target: LOG_TARGET,
					"Price oracle: {} ups, {} downs, price {} -> {}",
					ups, downs, current_price, new_price,
				);

				CurrentPrice::<T>::put(new_price);
				let block_number = frame_system::Pallet::<T>::block_number();
				let timestamp: u64 = T::TimeProvider::now().try_into().unwrap_or(0u64);
				T::OnPriceUpdate::on_price_update(new_price, block_number, timestamp);
			}

			NudgeCount::<T>::put(total_valid);
			Ok(())
		}

		/// Replace the set of active price feed endpoints.
		///
		/// Root-only. Accepts `(parsing_method_id, url_bytes)` pairs — the id
		/// is a `u8` handle for a [`ParsingMethod`] variant. Overwrites
		/// [`ActiveEndpoints`] wholesale. Fails with
		/// [`Error::TooManyEndpoints`], [`Error::UrlTooLong`], or
		/// [`Error::UnknownParsingMethod`] if the input is invalid.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(endpoints.len() as u64))]
		pub fn set_active_endpoints(
			origin: OriginFor<T>,
			endpoints: Vec<(u8, Vec<u8>)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			let converted: Vec<(ParsingMethod, BoundedUrl<T>)> = endpoints
				.into_iter()
				.map(|(id, url)| {
					let method = ParsingMethod::try_from(id)
						.map_err(|_| Error::<T>::UnknownParsingMethod)?;
					let url: BoundedUrl<T> =
						url.try_into().map_err(|_| Error::<T>::UrlTooLong)?;
					Ok((method, url))
				})
				.collect::<Result<_, Error<T>>>()?;
			let bounded: BoundedEndpoints<T> =
				converted.try_into().map_err(|_| Error::<T>::TooManyEndpoints)?;
			let count = bounded.len() as u32;
			ActiveEndpoints::<T>::put(bounded);
			Self::deposit_event(Event::ActiveEndpointsUpdated { count });
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let nudges = data
				.get_data::<PriceOracleInherentData>(&INHERENT_IDENTIFIER)
				.expect("Price oracle inherent data encoded correctly")
				.unwrap_or_default();

			Some(Call::submit_nudges { nudges })
		}

		fn check_inherent(call: &Self::Call, _data: &InherentData) -> Result<(), Self::Error> {
			let nudges = match call {
				Call::submit_nudges { ref nudges } => nudges,
				_ => return Ok(()),
			};

			let min = T::MinNudges::get();
			if (nudges.len() as u32) < min {
				return Err(InherentError::TooFewNudges(nudges.len() as u32, min));
			}

			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::submit_nudges { .. })
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn current_price() -> FixedU128 {
			CurrentPrice::<T>::get()
		}
	}
}
