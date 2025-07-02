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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{
		ClassifyDispatch, DispatchClass, DispatchResult, GetDispatchInfo, Pays, PaysFee, WeighData,
	},
	pallet_prelude::*,
	traits::IsSubType,
	weights::Weight,
};
use frame_support::{
	pallet_prelude::*,
	traits::{EnsureOrigin, Get},
};
use frame_system::{self, pallet_prelude::*};
use frame_system::{ensure_signed, pallet_prelude::BlockNumberFor};
use log::info;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		AtLeast32BitUnsigned, Bounded, CheckedAdd, DispatchInfoOf, DispatchOriginOf, Dispatchable,
		Member, One, SaturatedConversion, Saturating, TransactionExtension, ValidateResult, Zero,
	},
	transaction_validity::{InvalidTransaction, ValidTransaction},
};

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

/// Type alias for balance type from balances pallet.
// TODO: Remove use of balance pallet since it does not appear to be required
pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

/// Helper struct that requires approval from two origins.
pub struct AndGate<A, B>(PhantomData<(A, B)>);

/// Implementation of `EnsureOrigin` that requires approval from two different origins
/// to succeed. It creates a compound origin check where both origin A and origin B
/// must approve for overall check to pass. Used in asynchronous approval flow where
/// multiple origins need to independently approve a proposal over time.
impl<Origin, A, B> frame_support::traits::EnsureOrigin<Origin> for AndGate<A, B>
where
	Origin: Into<Result<frame_system::RawOrigin<Origin::AccountId>, Origin>>
		+ From<frame_system::RawOrigin<Origin::AccountId>>
		+ Clone,
	Origin: frame_support::traits::OriginTrait,
	A: EnsureOrigin<Origin, Success = ()>,
	B: EnsureOrigin<Origin, Success = ()>,
{
	type Success = ();

	fn try_origin(origin: Origin) -> Result<Self::Success, Origin> {
		let origin_clone = origin.clone();
		match A::try_origin(origin) {
			Ok(_) => B::try_origin(origin_clone),
			Err(_) => Err(origin_clone),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<Origin, ()> {
		// Placeholder implementation for benchmarking to create a successful origin from A
		A::try_successful_origin()
	}
}

struct WeightForSetDummy<T: pallet_balances::Config>(BalanceOf<T>);

impl<T: pallet_balances::Config> WeighData<(&BalanceOf<T>,)> for WeightForSetDummy<T> {
	fn weigh_data(&self, target: (&BalanceOf<T>,)) -> Weight {
		Weight::from_parts(100_000, 0)
	}
}

impl<T: pallet_balances::Config> ClassifyDispatch<(&BalanceOf<T>,)> for WeightForSetDummy<T> {
	fn classify_dispatch(&self, _target: (&BalanceOf<T>,)) -> DispatchClass {
		DispatchClass::Normal
	}
}

impl<T: pallet_balances::Config> PaysFee<(&BalanceOf<T>,)> for WeightForSetDummy<T> {
	fn pays_fee(&self, _target: (&BalanceOf<T>,)) -> Pays {
		Pays::Yes
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::WeightInfo;
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{Dispatchable, Hash, One};
	use sp_std::{fmt::Debug, marker::PhantomData};

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_balances::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		/// The hashing implementation.
		type Hashing: sp_runtime::traits::Hash;

		/// Identifier type for different origins that must maintain uniqueness and comparability.
		type OriginId: Parameter + Member + TypeInfo + Copy + Ord + MaxEncodedLen;

		/// The maximum number of approvals for a single proposal.
		#[pallet::constant]
		type MaxApprovals: Get<u32> + Clone;

		/// How long a proposal is valid for measured in blocks before it expires.
		#[pallet::constant]
		type ProposalLifetime: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	// Constant for the required number of approvals. The original specification by Dr Gavin Wood
	// requires exactly two approvals to satisfy the "AND Gate" pattern for two origins
	pub const REQUIRED_APPROVALS: u8 = 2;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Weight::zero()
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			// TODO
		}

		fn offchain_worker(_n: BlockNumberFor<T>) {
			// TODO
		}
	}

	impl<T: Config> Pallet<T> {
		/// Helper function to get error index of specific error variant
		fn error_index(error: Error<T>) -> u8 {
			match error {
				Error::ProposalAlreadyExists => 0,
				Error::ProposalNotFound => 1,
				Error::TooManyApprovals => 2,
				Error::NotAuthorized => 3,
				Error::ProposalAlreadyExecuted => 4,
				Error::ProposalExpired => 5,
				Error::AlreadyApproved => 6,
				Error::InsufficientApprovals => 7,
			}
		}

		/// Helper function to check if a proposal has sufficient approvals and execute it
		fn check_and_execute_proposal(
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			mut proposal_info: ProposalInfo<
				T::Hash,
				BlockNumberFor<T>,
				T::OriginId,
				T::MaxApprovals,
			>,
		) -> DispatchResult {
			// Check for minimum number of approvals (customize this logic based on your requirements)
			// Note that for the "AND Gate" pattern, we require a minimum of 2 approvals
			if proposal_info.approvals.len() >= REQUIRED_APPROVALS as usize {
				// Retrieve the actual call from storage
				if let Some(call) = <ProposalCalls<T>>::get(proposal_hash) {
					// Execute the call with root origin
					let result = call.dispatch(frame_system::RawOrigin::Root.into());

					// Update proposal status
					proposal_info.status = ProposalStatus::Executed;
					<Proposals<T>>::insert(proposal_hash, origin_id, proposal_info);

					// Clean up call data since it's no longer needed
					<ProposalCalls<T>>::remove(proposal_hash);

					// Emit event with the dispatch result
					Self::deposit_event(Event::ProposalExecuted {
						proposal_hash,
						origin_id,
						result: result.map(|_| ()).map_err(|e| e.error),
					});

					return Ok(());
				}

				return Err(Error::<T>::ProposalNotFound.into());
			}

			// Return an error when there aren't enough approvals
			Err(Error::<T>::InsufficientApprovals.into())
		}
	}

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Submit a proposal for approval, recording the first origin's approval.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::propose())]
		pub fn propose(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
			origin_id: T::OriginId,
			expiry: Option<BlockNumberFor<T>>,
		) -> DispatchResultWithPostInfo {
			// Check extrinsic was signed
			let who = ensure_signed(origin)?;

			// Compute hash of call for storage using system hashing implementation
			let proposal_hash = <T as frame_system::Config>::Hashing::hash_of(&call);

			// Check if given proposal already exists
			ensure!(
				!<Proposals<T>>::contains_key(&proposal_hash, &origin_id),
				Error::<T>::ProposalAlreadyExists
			);

			// Determine expiration block number if provided or otherwise use default
			let expiry_block = match expiry {
				Some(expiry_block) => Some(expiry_block),
				None => {
					// If no expiry was provided then use proposal lifetime config
					let current_block = frame_system::Pallet::<T>::block_number();
					Some(current_block.saturating_add(T::ProposalLifetime::get()))
				},
			};

			// Create an empty bounded vec for approvals
			let mut approvals = BoundedVec::<T::OriginId, T::MaxApprovals>::default();

			// Add proposer as first approval
			if let Err(_) = approvals.try_push(origin_id.clone()) {
				return Err(Error::<T>::TooManyApprovals.into());
			}

			// Create and store proposal metadata (bounded storage)
			let proposal_info = ProposalInfo {
				call_hash: proposal_hash,
				expiry: expiry_block,
				approvals,
				status: ProposalStatus::Pending,
			};

			// Store proposal metadata (bounded storage)
			<Proposals<T>>::insert(proposal_hash, origin_id.clone(), proposal_info);

			// Mark first approval in approvals storage (bounded)
			<Approvals<T>>::insert((proposal_hash, origin_id.clone()), origin_id.clone(), true);

			// Store actual call data (unbounded)
			<ProposalCalls<T>>::insert(proposal_hash, call);

			// Emit event
			Self::deposit_event(Event::ProposalCreated { proposal_hash, origin_id });

			Ok(().into())
		}

		/// Approve a previously submitted proposal.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::approve())]
		pub fn approve(
			origin: OriginFor<T>,
			call_hash: T::Hash,
			origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Try to fetch proposal from storage first
			let mut proposal_info =
				<Proposals<T>>::get(&call_hash, &origin_id).ok_or(Error::<T>::ProposalNotFound)?;

			// Check if proposal still pending
			if proposal_info.status != ProposalStatus::Pending {
				return match proposal_info.status {
					ProposalStatus::Executed => Err(Error::<T>::ProposalAlreadyExecuted.into()),
					ProposalStatus::Expired => Err(Error::<T>::ProposalExpired.into()),
					_ => Err(Error::<T>::ProposalNotFound.into()),
				};
			}

			// Check if proposal has expired
			if let Some(expiry) = proposal_info.expiry {
				let current_block = frame_system::Pallet::<T>::block_number();
				if current_block > expiry {
					proposal_info.status = ProposalStatus::Expired;
					<Proposals<T>>::insert(call_hash, origin_id, proposal_info);
					Self::deposit_event(Event::ProposalExpired {
						proposal_hash: call_hash,
						origin_id,
					});
					return Err(Error::<T>::ProposalExpired.into());
				}
			}

			// Check if origin_id already approved
			if <Approvals<T>>::contains_key((call_hash, origin_id), approving_origin_id) {
				return Err(Error::<T>::AlreadyApproved.into());
			}

			// Add to storage to mark that this origin is approved
			<Approvals<T>>::insert((call_hash, origin_id), approving_origin_id, true);

			// Add to proposal's approvals list if not yet present
			if !proposal_info.approvals.contains(&approving_origin_id) {
				if proposal_info.approvals.try_push(approving_origin_id).is_err() {
					return Err(Error::<T>::TooManyApprovals.into());
				}
			}

			// Update proposal in storage with new origin approval
			<Proposals<T>>::insert(call_hash, origin_id.clone(), &proposal_info);

			// Emit approval event
			Self::deposit_event(Event::ProposalApproved {
				proposal_hash: call_hash,
				origin_id: origin_id.clone(),
				approving_origin_id,
			});

			// Pass a clone of proposal info so original does not get modified if execution attempt fails
			match Self::check_and_execute_proposal(call_hash, origin_id, proposal_info.clone()) {
				// Success case results in proposal being executed
				Ok(_) => {},
				// Check if error is specifically the `InsufficientApprovals` error since we need
				// to silently ignore it when adding early approvals
				Err(e) => match e {
					DispatchError::Module(module_error) => {
						if module_error.index == <Self as PalletInfoAccess>::index() as u8 {
							let insufficient_approvals_index =
								Self::error_index(Error::<T>::InsufficientApprovals);

							// Propagate all errors except `InsufficientApprovals` error
							if module_error.error[0] != insufficient_approvals_index {
								return Err(DispatchError::Module(module_error).into());
							}
							// Otherwise silently ignore InsufficientApprovals error
						} else {
							// Error from another pallet must always be propagated
							return Err(DispatchError::Module(module_error).into());
						}
					},
					// Non-module errors must always be propagated
					_ => return Err(e.into()),
				},
			}

			Ok(().into())
		}

		/// A privileged call; in this case it resets our dummy value to something new.
		/// Implementation of a privileged call. The `origin` parameter is ROOT because
		/// it's not (directly) from an extrinsic, but rather the system as a whole has decided
		/// to execute it. Different runtimes have different reasons for allow privileged
		/// calls to be executed - we don't need to care why. Because it's privileged, we can
		/// assume it's a one-off operation and substantial processing/storage/memory can be used
		/// without worrying about gameability or attack scenarios.
		///
		/// The weight for this extrinsic we use our own weight object `WeightForSetDummy`
		/// or set_dummy() extrinsic to determine its weight
		#[pallet::call_index(2)]
		// #[pallet::weight(WeightForSetDummy::<T>(<BalanceOf<T>>::from(100u64.into())))]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		pub fn set_dummy(
			origin: OriginFor<T>,
			#[pallet::compact] new_value: T::Balance,
		) -> DispatchResult {
			ensure_root(origin)?;

			// Print out log or debug message in the console via log::{error, warn, info, debug,
			// trace}, accepting format strings similar to `println!`.
			// https://paritytech.github.io/substrate/master/sp_io/logging/fn.log.html
			// https://paritytech.github.io/substrate/master/frame_support/constant.LOG_TARGET.html
			info!("New value is now: {:?}", new_value);

			// Put the new value into storage.
			<Dummy<T>>::put(new_value);

			Self::deposit_event(Event::SetDummy { balance: new_value });

			// All good, no refund.
			Ok(())
		}

		// /// A dummy function for use in tests and benchmarks
		// #[pallet::call_index(3)]
		// #[pallet::weight(<T as pallet::Config>::WeightInfo::set_dummy())]
		// pub fn dummy_benchmark(
		//     origin: OriginFor<T>,
		//     remark: Vec<u8>,
		// ) -> DispatchResultWithPostInfo {
		//     ensure_signed(origin)?;
		//     Ok(().into())
		// }
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A proposal has been created.
		ProposalCreated {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
		},
		/// A proposal has been approved.
		ProposalApproved {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			approving_origin_id: T::OriginId,
		},
		/// A proposal has been executed.
		ProposalExecuted {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
			result: DispatchResult,
		},
		/// A proposal has expired.
		ProposalExpired {
			proposal_hash: T::Hash,
			origin_id: T::OriginId,
		},
		SetDummy {
			balance: BalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A proposal with these parameters already exists
		ProposalAlreadyExists,
		/// The proposal could not be found
		ProposalNotFound,
		/// The proposal has too many approvals
		TooManyApprovals,
		/// The caller is not authorized to approve
		NotAuthorized,
		/// The proposal has already been executed
		ProposalAlreadyExecuted,
		/// The proposal has expired
		ProposalExpired,
		/// The proposal is already approved
		AlreadyApproved,
		/// The proposal does not have enough approvals
		InsufficientApprovals,
	}

	/// Status of proposal
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum ProposalStatus {
		/// Proposal is pending and awaiting approvals
		Pending,
		/// Proposal has been executed
		Executed,
		/// Proposal has expired
		Expired,
	}

	/// Info about specific proposal
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(MaxApprovals))]
	pub struct ProposalInfo<Hash, BlockNumber, OriginId, MaxApprovals: Get<u32>> {
		/// The call hash of this proposal to execute
		pub call_hash: Hash,
		/// The block number after which this proposal expires
		pub expiry: Option<BlockNumber>,
		/// List of `OriginId`s that have approved this proposal
		pub approvals: BoundedVec<OriginId, MaxApprovals>,
		/// The current status of this proposal
		pub status: ProposalStatus,
	}

	/// Storage for proposals
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub type Proposals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::Hash,
		Blake2_128Concat,
		T::OriginId,
		ProposalInfo<T::Hash, BlockNumberFor<T>, T::OriginId, T::MaxApprovals>,
		OptionQuery,
	>;

	/// Storage for approvals by `OriginId`
	#[pallet::storage]
	#[pallet::getter(fn approvals)]
	pub type Approvals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		(T::Hash, T::OriginId),
		Blake2_128Concat,
		T::OriginId,
		bool,
		ValueQuery,
	>;

	/// Storage for calls themselves that is unbounded since
	/// `RuntimeCall` does not implement `MaxEncodedLen`
	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn proposal_calls)]
	pub type ProposalCalls<T: Config> =
		StorageMap<_, Identity, T::Hash, Box<<T as Config>::RuntimeCall>, OptionQuery>;

	#[pallet::storage]
	pub(super) type Dummy<T: Config> = StorageValue<_, T::Balance>;

	// The genesis config type.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		// TODO
		pub key: Option<T::AccountId>,
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// TODO
		}
	}
}
