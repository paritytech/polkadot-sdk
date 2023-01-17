// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Helper datatypes for cumulus. This includes the [`ParentAsUmp`] routing type which will route
//! messages into an [`UpwardMessageSender`] if the destination is `Parent`.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;
use cumulus_primitives_core::{MessageSendError, UpwardMessageSender};
use frame_support::{
	traits::{
		tokens::{fungibles, fungibles::Inspect},
		Get,
	},
	weights::Weight,
};
use polkadot_runtime_common::xcm_sender::ConstantPrice;
use sp_runtime::{traits::Saturating, SaturatedConversion};
use sp_std::{marker::PhantomData, prelude::*};
use xcm::{latest::prelude::*, WrapVersion};
use xcm_builder::TakeRevenue;
use xcm_executor::traits::{MatchesFungibles, TransactAsset, WeightTrader};

pub trait PriceForParentDelivery {
	fn price_for_parent_delivery(message: &Xcm<()>) -> MultiAssets;
}

impl PriceForParentDelivery for () {
	fn price_for_parent_delivery(_: &Xcm<()>) -> MultiAssets {
		MultiAssets::new()
	}
}

impl<T: Get<MultiAssets>> PriceForParentDelivery for ConstantPrice<T> {
	fn price_for_parent_delivery(_: &Xcm<()>) -> MultiAssets {
		T::get()
	}
}

/// Xcm router which recognises the `Parent` destination and handles it by sending the message into
/// the given UMP `UpwardMessageSender` implementation. Thus this essentially adapts an
/// `UpwardMessageSender` trait impl into a `SendXcm` trait impl.
///
/// NOTE: This is a pretty dumb "just send it" router; we will probably want to introduce queuing
/// to UMP eventually and when we do, the pallet which implements the queuing will be responsible
/// for the `SendXcm` implementation.
pub struct ParentAsUmp<T, W, P>(PhantomData<(T, W, P)>);
impl<T, W, P> SendXcm for ParentAsUmp<T, W, P>
where
	T: UpwardMessageSender,
	W: WrapVersion,
	P: PriceForParentDelivery,
{
	type Ticket = Vec<u8>;

	fn validate(
		dest: &mut Option<MultiLocation>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Vec<u8>> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;

		if d.contains_parents_only(1) {
			// An upward message for the relay chain.
			let xcm = msg.take().ok_or(SendError::MissingArgument)?;
			let price = P::price_for_parent_delivery(&xcm);
			let versioned_xcm =
				W::wrap_version(&d, xcm).map_err(|()| SendError::DestinationUnsupported)?;
			let data = versioned_xcm.encode();

			Ok((data, price))
		} else {
			// Anything else is unhandled. This includes a message that is not meant for us.
			// We need to make sure that dest/msg is not consumed here.
			*dest = Some(d);
			Err(SendError::NotApplicable)
		}
	}

	fn deliver(data: Vec<u8>) -> Result<XcmHash, SendError> {
		let (_, hash) = T::send_upward_message(data).map_err(|e| match e {
			MessageSendError::TooBig => SendError::ExceedsMaxMessageSize,
			e => SendError::Transport(e.into()),
		})?;

		Ok(hash)
	}
}

/// Contains information to handle refund/payment for xcm-execution
#[derive(Clone, Eq, PartialEq, Debug)]
struct AssetTraderRefunder {
	// The amount of weight bought minus the weigh already refunded
	weight_outstanding: Weight,
	// The concrete asset containing the asset location and outstanding balance
	outstanding_concrete_asset: MultiAsset,
}

/// Charges for execution in the first multiasset of those selected for fee payment
/// Only succeeds for Concrete Fungible Assets
/// First tries to convert the this MultiAsset into a local assetId
/// Then charges for this assetId as described by FeeCharger
/// Weight, paid balance, local asset Id and the multilocation is stored for
/// later refund purposes
/// Important: Errors if the Trader is being called twice by 2 BuyExecution instructions
/// Alternatively we could just return payment in the aforementioned case
pub struct TakeFirstAssetTrader<
	AccountId,
	FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
	Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
	ConcreteAssets: fungibles::Mutate<AccountId> + fungibles::Transfer<AccountId> + fungibles::Balanced<AccountId>,
	HandleRefund: TakeRevenue,
>(
	Option<AssetTraderRefunder>,
	PhantomData<(AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund)>,
);
impl<
		AccountId,
		FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
		Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
		ConcreteAssets: fungibles::Mutate<AccountId>
			+ fungibles::Transfer<AccountId>
			+ fungibles::Balanced<AccountId>,
		HandleRefund: TakeRevenue,
	> WeightTrader
	for TakeFirstAssetTrader<AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund>
{
	fn new() -> Self {
		Self(None, PhantomData)
	}
	// We take first multiasset
	// Check whether we can convert fee to asset_fee (is_sufficient, min_deposit)
	// If everything goes well, we charge.
	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: xcm_executor::Assets,
	) -> Result<xcm_executor::Assets, XcmError> {
		log::trace!(target: "xcm::weight", "TakeFirstAssetTrader::buy_weight weight: {:?}, payment: {:?}", weight, payment);

		// Make sure we dont enter twice
		if self.0.is_some() {
			return Err(XcmError::NotWithdrawable)
		}

		// We take the very first multiasset from payment
		// (assets are sorted by fungibility/amount after this conversion)
		let multiassets: MultiAssets = payment.clone().into();

		// Take the first multiasset from the selected MultiAssets
		let first = multiassets.get(0).ok_or(XcmError::AssetNotFound)?;

		// Get the local asset id in which we can pay for fees
		let (local_asset_id, _) =
			Matcher::matches_fungibles(&first).map_err(|_| XcmError::AssetNotFound)?;

		// Calculate how much we should charge in the asset_id for such amount of weight
		// Require at least a payment of minimum_balance
		// Necessary for fully collateral-backed assets
		let asset_balance: u128 = FeeCharger::charge_weight_in_fungibles(local_asset_id, weight)
			.map(|amount| {
				let minimum_balance = ConcreteAssets::minimum_balance(local_asset_id);
				if amount < minimum_balance {
					minimum_balance
				} else {
					amount
				}
			})?
			.try_into()
			.map_err(|_| XcmError::Overflow)?;

		// Convert to the same kind of multiasset, with the required fungible balance
		let required = first.id.clone().into_multiasset(asset_balance.into());

		// Substract payment
		let unused = payment.checked_sub(required.clone()).map_err(|_| XcmError::TooExpensive)?;

		// record weight and multiasset
		self.0 = Some(AssetTraderRefunder {
			weight_outstanding: weight,
			outstanding_concrete_asset: required,
		});

		Ok(unused)
	}

	fn refund_weight(&mut self, weight: Weight) -> Option<MultiAsset> {
		log::trace!(target: "xcm::weight", "TakeFirstAssetTrader::refund_weight weight: {:?}", weight);
		if let Some(AssetTraderRefunder {
			mut weight_outstanding,
			outstanding_concrete_asset: MultiAsset { id, fun },
		}) = self.0.clone()
		{
			// Get the local asset id in which we can refund fees
			let (local_asset_id, outstanding_balance) =
				Matcher::matches_fungibles(&(id.clone(), fun).into()).ok()?;

			let minimum_balance = ConcreteAssets::minimum_balance(local_asset_id);

			// Calculate asset_balance
			// This read should have already be cached in buy_weight
			let (asset_balance, outstanding_minus_substracted) =
				FeeCharger::charge_weight_in_fungibles(local_asset_id, weight).ok().map(
					|asset_balance| {
						// Require at least a drop of minimum_balance
						// Necessary for fully collateral-backed assets
						if outstanding_balance.saturating_sub(asset_balance) > minimum_balance {
							(asset_balance, outstanding_balance.saturating_sub(asset_balance))
						}
						// If the amount to be refunded leaves the remaining balance below ED,
						// we just refund the exact amount that guarantees at least ED will be
						// dropped
						else {
							(outstanding_balance.saturating_sub(minimum_balance), minimum_balance)
						}
					},
				)?;

			// Convert balances into u128
			let outstanding_minus_substracted: u128 =
				outstanding_minus_substracted.saturated_into();
			let asset_balance: u128 = asset_balance.saturated_into();

			// Construct outstanding_concrete_asset with the same location id and substracted balance
			let outstanding_concrete_asset: MultiAsset =
				(id.clone(), outstanding_minus_substracted).into();

			// Substract from existing weight and balance
			weight_outstanding = weight_outstanding.saturating_sub(weight);

			// Override AssetTraderRefunder
			self.0 = Some(AssetTraderRefunder { weight_outstanding, outstanding_concrete_asset });

			// Only refund if positive
			if asset_balance > 0 {
				Some((id, asset_balance).into())
			} else {
				None
			}
		} else {
			None
		}
	}
}

impl<
		AccountId,
		FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
		Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
		ConcreteAssets: fungibles::Mutate<AccountId>
			+ fungibles::Transfer<AccountId>
			+ fungibles::Balanced<AccountId>,
		HandleRefund: TakeRevenue,
	> Drop for TakeFirstAssetTrader<AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund>
{
	fn drop(&mut self) {
		if let Some(asset_trader) = self.0.clone() {
			HandleRefund::take_revenue(asset_trader.outstanding_concrete_asset);
		}
	}
}

/// XCM fee depositor to which we implement the TakeRevenue trait
/// It receives a Transact implemented argument, a 32 byte convertible acocuntId, and the fee receiver account
/// FungiblesMutateAdapter should be identical to that implemented by WithdrawAsset
pub struct XcmFeesTo32ByteAccount<FungiblesMutateAdapter, AccountId, ReceiverAccount>(
	PhantomData<(FungiblesMutateAdapter, AccountId, ReceiverAccount)>,
);
impl<
		FungiblesMutateAdapter: TransactAsset,
		AccountId: Clone + Into<[u8; 32]>,
		ReceiverAccount: frame_support::traits::Get<Option<AccountId>>,
	> TakeRevenue for XcmFeesTo32ByteAccount<FungiblesMutateAdapter, AccountId, ReceiverAccount>
{
	fn take_revenue(revenue: MultiAsset) {
		if let Some(receiver) = ReceiverAccount::get() {
			let ok = FungiblesMutateAdapter::deposit_asset(
				&revenue,
				&(X1(AccountId32 { network: None, id: receiver.into() }).into()),
				// We aren't able to track the XCM that initiated the fee deposit, so we create a
				// fake message hash here
				&XcmContext::with_message_hash([0; 32]),
			)
			.is_ok();

			debug_assert!(ok, "`deposit_asset` cannot generally fail; qed");
		}
	}
}

/// ChargeWeightInFungibles trait, which converts a given amount of weight
/// and an assetId, and it returns the balance amount that should be charged
/// in such assetId for that amount of weight
pub trait ChargeWeightInFungibles<AccountId, Assets: fungibles::Inspect<AccountId>> {
	fn charge_weight_in_fungibles(
		asset_id: <Assets as Inspect<AccountId>>::AssetId,
		weight: Weight,
	) -> Result<<Assets as Inspect<AccountId>>::Balance, XcmError>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::UpwardMessage;

	/// Validates [`validate`] for required Some(destination) and Some(message)
	struct OkFixedXcmHashWithAssertingRequiredInputsSender;
	impl OkFixedXcmHashWithAssertingRequiredInputsSender {
		const FIXED_XCM_HASH: [u8; 32] = [9; 32];

		fn fixed_delivery_asset() -> MultiAssets {
			MultiAssets::new()
		}

		fn expected_delivery_result() -> Result<(XcmHash, MultiAssets), SendError> {
			Ok((Self::FIXED_XCM_HASH, Self::fixed_delivery_asset()))
		}
	}
	impl SendXcm for OkFixedXcmHashWithAssertingRequiredInputsSender {
		type Ticket = ();

		fn validate(
			destination: &mut Option<MultiLocation>,
			message: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			assert!(destination.is_some());
			assert!(message.is_some());
			Ok(((), OkFixedXcmHashWithAssertingRequiredInputsSender::fixed_delivery_asset()))
		}

		fn deliver(_: Self::Ticket) -> Result<XcmHash, SendError> {
			Ok(Self::FIXED_XCM_HASH)
		}
	}

	/// Impl [`UpwardMessageSender`] that return `Other` error
	struct OtherErrorUpwardMessageSender;
	impl UpwardMessageSender for OtherErrorUpwardMessageSender {
		fn send_upward_message(_: UpwardMessage) -> Result<(u32, XcmHash), MessageSendError> {
			Err(MessageSendError::Other)
		}
	}

	#[test]
	fn parent_as_ump_does_not_consume_dest_or_msg_on_not_applicable() {
		// dummy message
		let message = Xcm(vec![Trap(5)]);

		// ParentAsUmp - check dest is really not applicable
		let dest = (Parent, Parent, Parent);
		let mut dest_wrapper = Some(dest.clone().into());
		let mut msg_wrapper = Some(message.clone());
		assert_eq!(
			Err(SendError::NotApplicable),
			<ParentAsUmp<(), (), ()> as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper)
		);

		// check wrapper were not consumed
		assert_eq!(Some(dest.clone().into()), dest_wrapper.take());
		assert_eq!(Some(message.clone()), msg_wrapper.take());

		// another try with router chain with asserting sender
		assert_eq!(
			OkFixedXcmHashWithAssertingRequiredInputsSender::expected_delivery_result(),
			send_xcm::<(ParentAsUmp<(), (), ()>, OkFixedXcmHashWithAssertingRequiredInputsSender)>(
				dest.into(),
				message
			)
		);
	}

	#[test]
	fn parent_as_ump_consumes_dest_and_msg_on_ok_validate() {
		// dummy message
		let message = Xcm(vec![Trap(5)]);

		// ParentAsUmp - check dest/msg is valid
		let dest = (Parent, Here);
		let mut dest_wrapper = Some(dest.clone().into());
		let mut msg_wrapper = Some(message.clone());
		assert!(<ParentAsUmp<(), (), ()> as SendXcm>::validate(
			&mut dest_wrapper,
			&mut msg_wrapper
		)
		.is_ok());

		// check wrapper were consumed
		assert_eq!(None, dest_wrapper.take());
		assert_eq!(None, msg_wrapper.take());

		// another try with router chain with asserting sender
		assert_eq!(
			Err(SendError::Transport("Other")),
			send_xcm::<(
				ParentAsUmp<OtherErrorUpwardMessageSender, (), ()>,
				OkFixedXcmHashWithAssertingRequiredInputsSender
			)>(dest.into(), message)
		);
	}
}
