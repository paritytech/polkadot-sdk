// Copyright (C) Parity Technologies (UK) Ltd.
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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Lets a parachain have a few XCM transact executed without payment.
//!
//! The pallet provides the XCM barrier [`AllowUnpaidTransact`], this barrier let XCM with the
//! instructions `UnpaidExecution` and `Transact` with `OriginKind::Native`, for the calls in
//! [`Config::AllowedCalls`] for the XCM execution weight at most [`Config::MaxWeight`], and at most
//! [`Config::MaxPerWindow`] of them per [`Config::Window`], be executed without payment.
//!
//! The storage footprint is paid by the parachain deposit, for offboarded parachains it is removed
//! lazily with the free call [`Call::remove_offboarded`] triggered from the offchain worker.

use crate::paras;
use core::marker::PhantomData;
use frame_support::{
	ensure,
	pallet_prelude::*,
	traits::{Contains, ProcessMessageError},
};
use frame_system::{
	offchain::{CreateAuthorizedTransaction, SubmitTransaction},
	pallet_prelude::*,
};
use polkadot_parachain_primitives::primitives::IsSystem;
use polkadot_primitives::Id as ParaId;
use sp_runtime::traits::{Saturating, Zero};
use xcm::latest::prelude::*;
use xcm_executor::traits::{Properties, ShouldExecute};

pub use pallet::*;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

const LOG_TARGET: &str = "runtime::unpaid-transact";

pub trait WeightInfo {
	fn remove_offboarded() -> Weight;
	fn authorize_remove_offboarded() -> Weight;
}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

impl WeightInfo for TestWeightInfo {
	fn remove_offboarded() -> Weight {
		Weight::MAX
	}
	fn authorize_remove_offboarded() -> Weight {
		Weight::MAX
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config + paras::Config + CreateAuthorizedTransaction<Call<Self>>
	{
		/// The calls a parachain gets dispatched without payment.
		type AllowedCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;

		/// The heaviest message a parachain gets executed without payment, as the XCM executor
		/// weighs it.
		type MaxWeight: Get<Weight>;

		/// How many messages a parachain gets executed without payment in one
		/// [`Config::Window`].
		#[pallet::constant]
		type MaxPerWindow: Get<u32>;

		/// The length, in blocks, of the window over which the messages of a parachain are
		/// counted.
		///
		/// A parachain's first window starts at its first message. Once a window is over, the
		/// next one starts at the parachain's next message.
		#[pallet::constant]
		type Window: Get<BlockNumberFor<Self>>;

		/// How often, in blocks, the offchain worker submits [`Call::remove_offboarded`] for the
		/// parachains that are offboarded. Zero for never.
		#[pallet::constant]
		type CleanupPeriod: Get<BlockNumberFor<Self>>;

		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
	}

	/// For each parachain, the block at which its current window started and the number of
	/// messages it got executed without payment since then.
	///
	/// The entry of a parachain stays once its window is over, and once the parachain is
	/// offboarded, until [`Call::remove_offboarded`] removes it.
	#[pallet::storage]
	pub type Windows<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, (BlockNumberFor<T>, u32), OptionQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// A window of no block is over at once: nothing would be counted.
			assert!(!T::Window::get().is_zero(), "`Window` must not be zero");
		}

		fn offchain_worker(now: BlockNumberFor<T>) {
			let period = T::CleanupPeriod::get();
			if period.is_zero() || !(now % period).is_zero() {
				return;
			}
			for para in Windows::<T>::iter_keys().filter(Self::is_offboarded) {
				let call = Call::remove_offboarded { para };
				let xt = T::create_authorized_transaction(call.into());
				if SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_err() {
					log::debug!(target: LOG_TARGET, "Failed to submit `remove_offboarded`");
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Removes the window of `para`.
		///
		/// Anybody can submit it, as a general transaction without a signature. It is valid if
		/// `para` is offboarded and has a window.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_offboarded())]
		#[pallet::weight_of_authorize(<T as Config>::WeightInfo::authorize_remove_offboarded())]
		#[pallet::authorize(Self::authorize_remove_offboarded)]
		pub fn remove_offboarded(origin: OriginFor<T>, para: ParaId) -> DispatchResult {
			ensure_authorized(origin)?;
			Windows::<T>::remove(para);
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Counts one more message in the window of `para`. A new window starts now if `para` has
	/// none or its last one is over.
	///
	/// Fails, writing nothing, if `para` already has [`Config::MaxPerWindow`] messages in its
	/// window.
	fn try_count(para: ParaId) -> Result<(), ()> {
		let now = frame_system::Pallet::<T>::block_number();
		let (start, used) = match Windows::<T>::get(para) {
			Some((start, used)) if now.saturating_sub(start) < T::Window::get() => (start, used),
			_ => (now, 0),
		};
		ensure!(used < T::MaxPerWindow::get(), ());
		Windows::<T>::insert(para, (start, used.saturating_add(1)));
		Ok(())
	}
}

impl<T: Config> Pallet<T> {
	/// Whether `para` is not registered at all: neither onboarding, onboarded nor offboarding.
	fn is_offboarded(para: &ParaId) -> bool {
		paras::Pallet::<T>::lifecycle(*para).is_none()
	}

	/// A [`Call::remove_offboarded`] is valid if it removes a window, of a parachain that is
	/// offboarded. It has the lowest priority: nothing waits for it.
	fn authorize_remove_offboarded(
		_source: TransactionSource,
		para: &ParaId,
	) -> TransactionValidityWithRefund {
		if !Windows::<T>::contains_key(para) || !Self::is_offboarded(para) {
			return Err(InvalidTransaction::Stale.into());
		}
		ValidTransaction::with_tag_prefix("UnpaidTransactRemoveOffboarded")
			.priority(TransactionPriority::MIN)
			.and_provides(para)
			.longevity(64)
			.propagate(true)
			.build()
			.map(|validity| (validity, Weight::zero()))
	}
}

/// Allows a child parachain that is not a system one to have one call in
/// [`Config::AllowedCalls`] dispatched as itself without payment, [`Config::MaxPerWindow`] times
/// per [`Config::Window`].
///
/// The message must weigh at most [`Config::MaxWeight`] and be `UnpaidExecution`, with no weight
/// limit or one of at least the weight of the message, followed by one `Transact` with
/// `OriginKind::Native`.
///
/// A message that is let through is counted, whether its call succeeds or not.
pub struct AllowUnpaidTransact<T>(PhantomData<T>);
impl<T: Config> ShouldExecute for AllowUnpaidTransact<T> {
	fn should_execute<RuntimeCall>(
		origin: &Location,
		instructions: &mut [Instruction<RuntimeCall>],
		max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		let para = match origin.unpack() {
			(0, [Parachain(id)]) => ParaId::from(*id),
			_ => return Err(ProcessMessageError::Unsupported),
		};
		ensure!(!para.is_system(), ProcessMessageError::Unsupported);
		ensure!(max_weight.all_lte(T::MaxWeight::get()), ProcessMessageError::Unsupported);
		let [UnpaidExecution { weight_limit, .. }, Transact { origin_kind, call, .. }] =
			instructions
		else {
			return Err(ProcessMessageError::Unsupported);
		};
		ensure!(*origin_kind == OriginKind::Native, ProcessMessageError::Unsupported);
		let is_covered = match weight_limit {
			Unlimited => true,
			Limited(limit) => limit.all_gte(max_weight),
		};
		ensure!(is_covered, ProcessMessageError::Unsupported);
		// `RuntimeCall` is not known to be decodable here: decode the call as the runtime's.
		//
		// This decodes the call, which the executor does not weigh for a barrier.
		// It may be abusive.
		let call: <T as frame_system::Config>::RuntimeCall = call
			.clone()
			.transmute_encoded()
			.try_into()
			.map_err(|_| ProcessMessageError::BadFormat)?;
		ensure!(T::AllowedCalls::contains(&call), ProcessMessageError::Unsupported);
		// Last: a message that is refused for another reason is not counted.
		//
		// This reads and may write storage, which the executor does not weigh for a barrier.
		// It may be abusive.
		Pallet::<T>::try_count(para).map_err(|_| ProcessMessageError::Unsupported)
	}
}
