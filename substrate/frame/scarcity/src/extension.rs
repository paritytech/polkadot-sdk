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

//! Scarcity transaction extension.
//!
//! # Runtime ordering
//!
//! [`AsScarcity`] changes a signed origin into [`Origin::Nft`] during validation. Following
//! Coinage's purse model, runtime integrators must place it with the origin modifiers, **before**
//! `frame_system::AuthorizeCall`, signed-account checks, and transaction payment:
//!
//! ```text
//! AsScarcity -> AuthorizeCall -> ... -> CheckNonce -> ... ->
//!     SkipCheckIfFeeless<ChargeAssetTxPayment>
//! ```
//!
//! The account checks deliberately see a non-system origin and skip it, so an NFT-only purse needs
//! neither a System account nor a balance. [`AsScarcity`] consumes the NFT during preparation to
//! prevent concurrent use. A successful move increments its state nonce. Failed dispatch restores
//! the same state and applies a temporary backoff lock; once that lock expires, the same signed
//! transaction can be submitted again. This is the same retry model as Coinage.
//!
//! Placing this extension after payment prevents
//! `pallet_skip_feeless_payment::SkipCheckIfFeeless` from observing [`Origin::Nft`] and makes a
//! balance-less purse unable to transact.
//!
//! # Replay and mortality
//!
//! Purse authorization is not account-nonce-based: a signed NFT transaction stays valid for as
//! long as its purse still holds the named instance at the named state nonce. Two rules bound
//! stale intent, exactly as in Coinage:
//!
//! * Callers must sign **mortal** transactions with an era shorter than [`Config::LockPeriod`]. A
//!   successful move invalidates every outstanding authorization by incrementing the state nonce,
//!   but an unexecuted transaction is otherwise replayable by anyone who has seen it until its era
//!   expires.
//! * Because the era ends before the shortest failure lock does, a failed transaction can never
//!   re-enter a block: every retry after a failure is a fresh signing decision rather than a
//!   third-party replay of the old transaction.
//!
//! A holder can also cancel an outstanding authorization at any time by moving the NFT, which
//! increments its state nonce.

use crate::{pallet::*, weights::WeightInfo, Config, Nft};
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::{Get, TransactionSource},
	traits::{IsSubType, OriginTrait, UnixTime},
	weights::Weight,
	CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		DispatchInfoOf, Implication, PostDispatchInfoOf, TransactionExtension, ValidateResult,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	DispatchResult,
};

/// The Scarcity authorization requested by this extension.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	EqNoBound,
	CloneNoBound,
	PartialEqNoBound,
	DecodeWithMemTracking,
	DebugNoBound,
)]
pub enum AsScarcityInfo {
	/// Authorize a transfer or burn using the NFT's current purse-key state.
	AsNft {
		/// The permanent instance expected at the purse key.
		instance: InstanceId,
		/// The ownership-state revision expected for that instance.
		state_nonce: u64,
	},
}

/// An error reported while validating [`AsScarcity`].
#[repr(u8)]
pub enum CustomInvalidity {
	/// The purse-key authorization must begin with a signed origin.
	OriginToAsNftMustBeSigned = 0,
	/// The purse key is temporarily locked after a failed dispatch.
	NftTemporarilyLocked = 1,
	/// The purse key has no NFT to authorize the requested action.
	NoNft = 2,
	/// The transfer destination equals the current purse key.
	TransferToSelf = 3,
	/// The transfer destination already holds an NFT.
	DestinationOccupied = 4,
	/// The purse key holds a different instance or ownership-state revision.
	NftStateMismatch = 5,
}

impl From<CustomInvalidity> for TransactionValidityError {
	fn from(value: CustomInvalidity) -> Self {
		InvalidTransaction::Custom(value as u8).into()
	}
}

/// Information carried from validation to preparation.
pub enum Val<T: Config + Send + Sync> {
	NotUsing,
	UsingNft { owner: T::AccountId, instance: InstanceId, state_nonce: u64 },
}

/// Information carried from preparation to post-dispatch.
pub enum Pre<T: Config + Send + Sync> {
	NotUsing,
	UsingNft { owner: T::AccountId, nft: Nft },
}

/// Purse-key authorization for Scarcity transfers and burns.
///
/// An authorization names the permanent instance and its ownership-state nonce. The instance
/// identifier prevents an authorization from acting on a different NFT if a purse key is reused,
/// while the state nonce invalidates it if the same instance is moved away and later returned.
/// Like Coinage, failed dispatch restores the same purse state behind a temporary lock rather than
/// consuming a System account nonce.
///
/// Runtime ordering is security-critical: place this extension before
/// `frame_system::AuthorizeCall`, signed-account checks, and transaction payment. See the
/// [module-level ordering requirements](crate::extension#runtime-ordering).
#[derive(
	Encode,
	Decode,
	TypeInfo,
	EqNoBound,
	CloneNoBound,
	PartialEqNoBound,
	DefaultNoBound,
	DecodeWithMemTracking,
	DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct AsScarcity<T: Config + Send + Sync>(Option<AsScarcityInfo>, PhantomData<T>);

impl<T: Config + Send + Sync> AsScarcity<T> {
	/// Create an extension. `None` is the identity extension for ordinary transactions.
	pub fn new(explicit: Option<AsScarcityInfo>) -> Self {
		Self(explicit, PhantomData)
	}

	fn failed_dispatch_lock(previous: Option<LockInfo>) -> LockInfo {
		let retries = previous.map(|lock| lock.retries.saturating_add(1)).unwrap_or(1);
		let exponent = retries.saturating_sub(1);
		let multiplier = 2u64.saturating_pow(u32::from(exponent).min(63));
		LockInfo {
			retries,
			until: T::UnixTime::now()
				.as_secs()
				.saturating_add(multiplier.saturating_mul(T::LockPeriod::get())),
		}
	}
}

impl<T: Config + Send + Sync> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for AsScarcity<T>
{
	const IDENTIFIER: &'static str = "AsScarcity";
	type Implicit = ();
	type Val = Val<T>;
	type Pre = Pre<T>;

	fn weight(&self, call: &<T as frame_system::Config>::RuntimeCall) -> Weight {
		if matches!(self.0.as_ref(), Some(AsScarcityInfo::AsNft { .. })) &&
			matches!(
				call.is_sub_type(),
				Some(Call::<T>::transfer { .. }) | Some(Call::<T>::burn {})
			) {
			T::WeightInfo::as_scarcity_pipeline()
		} else {
			Weight::zero()
		}
	}

	fn validate(
		&self,
		mut origin: <T as frame_system::Config>::RuntimeOrigin,
		call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Implication,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
		let transfer_to = match call.is_sub_type() {
			Some(Call::<T>::transfer { to }) => Some(to),
			Some(Call::<T>::burn {}) => None,
			_ => return Ok((ValidTransaction::default(), Val::NotUsing, origin)),
		};
		let Some(AsScarcityInfo::AsNft { instance, state_nonce }) = self.0.as_ref() else {
			return Ok((ValidTransaction::default(), Val::NotUsing, origin));
		};

		let Some(frame_system::Origin::<T>::Signed(owner)) = origin.as_system_ref() else {
			return Err(CustomInvalidity::OriginToAsNftMustBeSigned.into());
		};
		let owner = owner.clone();
		let now = T::UnixTime::now().as_secs();
		if let Some(lock) = Locked::<T>::get(&owner) {
			if lock.until > now {
				return Err(CustomInvalidity::NftTemporarilyLocked.into());
			}
		}
		let nft = NftsByOwner::<T>::get(&owner).ok_or(CustomInvalidity::NoNft)?;
		if nft.instance != *instance || nft.state_nonce != *state_nonce {
			return Err(CustomInvalidity::NftStateMismatch.into());
		}
		if let Some(to) = transfer_to {
			// Pre-validate the destination so ordinary user error is rejected at the pool and
			// never reaches dispatch, where a failure triggers the backoff lock. The
			// dispatch-time checks remain for genuine same-block races. Mirrors coinage's
			// `validate_transfer` pattern. Burns have no destination checks.
			if to == &owner {
				return Err(CustomInvalidity::TransferToSelf.into());
			}
			if NftsByOwner::<T>::contains_key(to) {
				return Err(CustomInvalidity::DestinationOccupied.into());
			}
		}
		let priority = now.saturating_sub(nft.last_moved).min(T::MaxTransferPriority::get());
		let validity = ValidTransaction::with_tag_prefix("Scarcity")
			.and_provides((nft.instance, nft.state_nonce))
			.priority(priority)
			.into();
		origin.set_caller_from(Origin::Nft { owner: owner.clone(), nft });
		Ok((
			validity,
			Val::UsingNft { owner, instance: *instance, state_nonce: *state_nonce },
			origin,
		))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<T as frame_system::Config>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Val::NotUsing => Ok(Pre::NotUsing),
			Val::UsingNft { owner, instance, state_nonce } => {
				let nft = NftsByOwner::<T>::try_mutate_exists(
					&owner,
					|maybe_nft| -> Result<Nft, TransactionValidityError> {
						let nft = maybe_nft.as_ref().ok_or(CustomInvalidity::NoNft)?;
						if nft.instance != instance || nft.state_nonce != state_nonce {
							return Err(CustomInvalidity::NftStateMismatch.into());
						}
						// Dispatch assumes the source purse is empty. Taking the NFT here
						// prevents same-block double use and lets post-dispatch restore the exact
						// pre-state if dispatch fails.
						Ok(maybe_nft.take().expect("NFT existence checked above; qed"))
					},
				)?;
				Ok(Pre::UsingNft { owner, nft })
			},
		}
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		if let Pre::UsingNft { owner, nft } = pre {
			if result.is_err() {
				NftsByOwner::<T>::insert(&owner, nft);
				Locked::<T>::insert(&owner, Self::failed_dispatch_lock(Locked::<T>::get(&owner)));
			} else {
				Locked::<T>::remove(&owner);
			}
		}
		Ok(Weight::zero())
	}
}
