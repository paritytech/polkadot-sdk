// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Helper datatypes for cumulus. This includes the [`ParentAsUmp`] routing type which will route
//! messages into an [`UpwardMessageSender`] if the destination is `Parent`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::Encode;
use core::marker::PhantomData;
use cumulus_primitives_core::{MessageSendError, UpwardMessageSender};
use frame_support::{
	defensive,
	traits::{tokens::fungibles, Contains, Get, OnUnbalanced as OnUnbalancedT},
	weights::{Weight, WeightToFee as WeightToFeeT},
	CloneNoBound,
};
use pallet_asset_conversion::{QuotePrice as QuotePriceT, SwapCredit as SwapCreditT};
use polkadot_runtime_common::xcm_sender::PriceForMessageDelivery;
use sp_runtime::{
	traits::{CheckedSub, MaybeEquivalence, Zero},
	SaturatedConversion,
};
use xcm::{latest::prelude::*, VersionedLocation, VersionedXcm, WrapVersion};
use xcm_builder::{InspectMessageQueues, TakeRevenue};
use xcm_executor::{
	traits::{MatchesFungibles, TransactAsset, WeightFee, WeightTrader},
	AssetsInHolding,
};

#[cfg(test)]
mod tests;

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
	P: PriceForMessageDelivery<Id = ()>,
{
	type Ticket = Vec<u8>;

	fn validate(dest: &mut Option<Location>, msg: &mut Option<Xcm<()>>) -> SendResult<Vec<u8>> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;

		if d.contains_parents_only(1) {
			// An upward message for the relay chain.
			let xcm = msg.take().ok_or(SendError::MissingArgument)?;
			let price = P::price_for_delivery((), &xcm);
			let versioned_xcm =
				W::wrap_version(&d, xcm).map_err(|()| SendError::DestinationUnsupported)?;
			versioned_xcm
				.check_is_decodable()
				.map_err(|()| SendError::ExceedsMaxMessageSize)?;
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

impl<T: UpwardMessageSender + InspectMessageQueues, W, P> InspectMessageQueues
	for ParentAsUmp<T, W, P>
{
	fn clear_messages() {
		T::clear_messages();
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		T::get_messages()
	}
}

/// Contains information to handle refund/payment for xcm-execution
#[derive(Clone, Eq, PartialEq, Debug)]
struct AssetTraderRefunder {
	// The amount of weight bought minus the weigh already refunded
	weight_outstanding: Weight,
	// The concrete asset containing the asset location and outstanding balance
	outstanding_concrete_asset: Asset,
}

// // FIXME docs
// /// Charges for execution in the first asset of those selected for fee payment
// /// Only succeeds for Concrete Fungible Assets
// /// First tries to convert the this Asset into a local assetId
// /// Then charges for this assetId as described by FeeCharger
// /// Weight, paid balance, local asset Id and the location is stored for
// /// later refund purposes
// /// Important: Errors if the Trader is being called twice by 2 BuyExecution instructions
// /// Alternatively we could just return payment in the aforementioned case
pub struct ConcreteAssetTrader<
	AccountId,
	AssetIdConversion: MaybeEquivalence<Location, ConcreteAssets::AssetId>,
	AcceptableAssets: Contains<Location>,
	ConcreteAssets: fungibles::Inspect<AccountId>,
	FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
	TakeFee: TakeRevenue,
>(
	PhantomData<(
		AccountId,
		AssetIdConversion,
		AcceptableAssets,
		ConcreteAssets,
		FeeCharger,
		TakeFee,
	)>,
);
impl<
		AccountId,
		AssetIdConversion: MaybeEquivalence<Location, ConcreteAssets::AssetId>,
		AcceptableAssets: Contains<Location>,
		ConcreteAssets: fungibles::Inspect<AccountId>,
		FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
		TakeFee: TakeRevenue,
	> WeightTrader
	for ConcreteAssetTrader<
		AccountId,
		AssetIdConversion,
		AcceptableAssets,
		ConcreteAssets,
		FeeCharger,
		TakeFee,
	>
{
	fn weight_fee(
		weight: &Weight,
		desired_asset_id: &AssetId,
		context: Option<&XcmContext>,
	) -> Result<WeightFee, XcmError> {
		// TODO logs
		if !AcceptableAssets::contains(&desired_asset_id.0) {
			return Err(XcmError::FeesNotMet);
		}

		let concrete_asset_id =
			AssetIdConversion::convert(&desired_asset_id.0).ok_or(XcmError::FeesNotMet)?;

		let required_amount =
			FeeCharger::charge_weight_in_fungibles(concrete_asset_id.clone(), weight.clone())
				.map(|amount| {
					let minimum_balance = ConcreteAssets::minimum_balance(concrete_asset_id);
					if amount < minimum_balance {
						minimum_balance
					} else {
						amount
					}
				})?
				.try_into()
				.map_err(|_| XcmError::Overflow)?;

		Ok(WeightFee::Desired(required_amount))
	}

	fn refund_amount(
		weight: &Weight,
		used_asset_id: &AssetId,
		paid_amount: u128,
		context: Option<&XcmContext>,
	) -> Option<u128> {
		// TODO logs
		if !AcceptableAssets::contains(&used_asset_id.0) {
			return None;
		}

		let concrete_asset_id = AssetIdConversion::convert(&used_asset_id.0)?;

		let refund_amount =
			FeeCharger::charge_weight_in_fungibles(concrete_asset_id.clone(), weight.clone())
				.map(|amount| {
					// TODO explain why this should always succeed
					let paid_amount: ConcreteAssets::Balance = paid_amount.try_into().ok()?;

					let resulting_paid_amount = paid_amount.checked_sub(&amount)?;

					let minimum_balance = ConcreteAssets::minimum_balance(concrete_asset_id);

					if resulting_paid_amount >= minimum_balance {
						Some(amount)
					} else {
						// ensure refund results in at least minimum_balance weight fee
						let correction = minimum_balance - resulting_paid_amount;
						Some(amount - correction)
					}
				})
				.ok()
				.flatten()?;

		let refund_amount: u128 = refund_amount.try_into().ok()?;

		(refund_amount != 0).then_some(refund_amount)
	}

	fn take_fee(asset_id: &AssetId, amount: u128) -> bool {
		if AcceptableAssets::contains(&asset_id.0) {
			TakeFee::take_revenue((asset_id.clone(), amount).into());
			true
		} else {
			false
		}
	}
}

/// XCM fee depositor to which we implement the `TakeRevenue` trait.
/// It receives a `Transact` implemented argument and a 32 byte convertible `AccountId`, and the fee
/// receiver account's `FungiblesMutateAdapter` should be identical to that implemented by
/// `WithdrawAsset`.
pub struct XcmFeesTo32ByteAccount<FungiblesMutateAdapter, AccountId, ReceiverAccount>(
	PhantomData<(FungiblesMutateAdapter, AccountId, ReceiverAccount)>,
);
impl<
		FungiblesMutateAdapter: TransactAsset,
		AccountId: Clone + Into<[u8; 32]>,
		ReceiverAccount: Get<Option<AccountId>>,
	> TakeRevenue for XcmFeesTo32ByteAccount<FungiblesMutateAdapter, AccountId, ReceiverAccount>
{
	fn take_revenue(revenue: Asset) {
		if let Some(receiver) = ReceiverAccount::get() {
			let ok = FungiblesMutateAdapter::deposit_asset(
				&revenue,
				&([AccountId32 { network: None, id: receiver.into() }].into()),
				None,
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
		asset_id: <Assets as fungibles::Inspect<AccountId>>::AssetId,
		weight: Weight,
	) -> Result<<Assets as fungibles::Inspect<AccountId>>::Balance, XcmError>;
}

// FIXME docs
/// Provides an implementation of [`WeightTrader`] to charge for weight using the first asset
/// specified in the `payment` argument.
///
/// The asset used to pay for the weight must differ from the `Target` asset and be exchangeable for
/// the same `Target` asset through `SwapCredit`.
///
/// ### Parameters:
/// - `Target`: the asset into which the user's payment will be exchanged using `SwapCredit`.
/// - `SwapCredit`: mechanism used for the exchange of the user's payment asset into the `Target`.
/// - `WeightToFee`: weight to the `Target` asset fee calculator.
/// - `Fungibles`: registry of fungible assets.
/// - `FungiblesAssetMatcher`: utility for mapping [`Asset`] to `Fungibles::AssetId` and
///   `Fungibles::Balance`.
/// - `OnUnbalanced`: handler for the fee payment.
/// - `AccountId`: the account identifier type.
pub struct SwapAssetTrader<
	Target: Get<Location>,
	AssetIdConversion: MaybeEquivalence<Location, QuotePrice::AssetKind>,
	Swappable: Contains<Location>,
	WeightToFee: WeightToFeeT,
	QuotePrice: QuotePriceT<Balance = WeightToFee::Balance>,
>(PhantomData<(Target, AssetIdConversion, Swappable, WeightToFee, QuotePrice)>);

impl<
		Target: Get<Location>,
		AssetIdConversion: MaybeEquivalence<Location, QuotePrice::AssetKind>,
		Swappable: Contains<Location>,
		WeightToFee: WeightToFeeT,
		QuotePrice: QuotePriceT<Balance = WeightToFee::Balance>,
	> WeightTrader for SwapAssetTrader<Target, AssetIdConversion, Swappable, WeightToFee, QuotePrice>
{
	fn weight_fee(
		weight: &Weight,
		asset_id: &AssetId,
		context: Option<&XcmContext>,
	) -> Result<WeightFee, XcmError> {
		log::trace!(
			target: "xcm::weight",
			"SwapAssetTrader::weight_price weight: {:?}, asset_id: {:?}, context: {:?}",
			weight,
			asset_id,
			context,
		);

		let required_asset_id = AssetId(Target::get());
		if required_asset_id.eq(asset_id) {
			log::trace!(
				target: "xcm::weight",
				"SwapAssetTrader::weight_price asset is same as the Target, won't replace, skipping.",
			);
			// current trader is not applicable.
			return Err(XcmError::FeesNotMet);
		}

		if !Swappable::contains(&asset_id.0) {
			log::trace!(
				target: "xcm::weight",
				"SwapAssetTrader::weight_price asset isn't swappable",
			);
			return Err(XcmError::FeesNotMet);
		}

		let required_asset_kind = AssetIdConversion::convert(&required_asset_id.0)
			.ok_or(XcmError::FeesNotMet)
			.inspect_err(|_| {
				log::trace!(
					target: "xcm::weight",
					"SwapAssetTrader::weight_price unable to convert required asset id to asset kind"
				)
			})?;

		let desired_asset_kind = AssetIdConversion::convert(&asset_id.0)
			.ok_or(XcmError::FeesNotMet)
			.inspect_err(|_| {
				log::trace!(
					target: "xcm::weight",
					"SwapAssetTrader::weight_price unable to convert desired asset id to asset kind"
				)
			})?;

		let required_amount = WeightToFee::weight_to_fee(weight);
		let required_amount_u128 = required_amount.try_into().map_err(|_| XcmError::Overflow)?;

		let swap_amount = QuotePrice::quote_price_tokens_for_exact_tokens(
			desired_asset_kind,
			required_asset_kind,
			required_amount,
			true,
		)
		.ok_or(XcmError::FeesNotMet)
		.inspect_err(|_| {
			log::trace!(
				target: "xcm::weight",
				"SwapAssetTrader::weight_price unable to quote the swap price"
			)
		})?
		.try_into()
		.map_err(|_| XcmError::Overflow)?;

		Ok(WeightFee::Swap {
			required_fee: (required_asset_id, required_amount_u128).into(),
			swap_amount,
		})
	}

	fn refund_amount(
		_weight: &Weight,
		_used_asset_id: &AssetId,
		_paid_amount: u128,
		_context: Option<&XcmContext>,
	) -> Option<u128> {
		// FIXME explain
		None
	}

	fn take_fee(_asset_id: &AssetId, _amount: u128) -> bool {
		// FIXME explain
		false
	}
}

#[cfg(test)]
mod test_xcm_router {
	use super::*;
	use cumulus_primitives_core::UpwardMessage;
	use frame_support::assert_ok;
	use xcm::MAX_XCM_DECODE_DEPTH;

	/// Validates [`validate`] for required Some(destination) and Some(message)
	struct OkFixedXcmHashWithAssertingRequiredInputsSender;
	impl OkFixedXcmHashWithAssertingRequiredInputsSender {
		const FIXED_XCM_HASH: [u8; 32] = [9; 32];

		fn fixed_delivery_asset() -> Assets {
			Assets::new()
		}

		fn expected_delivery_result() -> Result<(XcmHash, Assets), SendError> {
			Ok((Self::FIXED_XCM_HASH, Self::fixed_delivery_asset()))
		}
	}
	impl SendXcm for OkFixedXcmHashWithAssertingRequiredInputsSender {
		type Ticket = ();

		fn validate(
			destination: &mut Option<Location>,
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
		let mut dest_wrapper = Some(dest.into());
		let mut msg_wrapper = Some(message.clone());
		assert_eq!(
			Err(SendError::NotApplicable),
			<ParentAsUmp<(), (), ()> as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper)
		);

		// check wrapper were not consumed
		assert_eq!(Some(dest.into()), dest_wrapper.take());
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

	#[test]
	fn parent_as_ump_validate_nested_xcm_works() {
		let dest = Parent;

		type Router = ParentAsUmp<(), (), ()>;

		// Message that is not too deeply nested:
		let mut good = Xcm(vec![ClearOrigin]);
		for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
			good = Xcm(vec![SetAppendix(good)]);
		}

		// Check that the good message is validated:
		assert_ok!(<Router as SendXcm>::validate(&mut Some(dest.into()), &mut Some(good.clone())));

		// Nesting the message one more time should reject it:
		let bad = Xcm(vec![SetAppendix(good)]);
		assert_eq!(
			Err(SendError::ExceedsMaxMessageSize),
			<Router as SendXcm>::validate(&mut Some(dest.into()), &mut Some(bad))
		);
	}
}

/// Implementation of `xcm_builder::EnsureDelivery` which helps to ensure delivery to the
/// parent relay chain. Deposits existential deposit for origin (if needed).
/// Deposits estimated fee to the origin account (if needed).
/// Allows triggering of additional logic for a specific `ParaId` (e.g. to open an HRMP channel) if
/// needed.
#[cfg(feature = "runtime-benchmarks")]
pub struct ToParentDeliveryHelper<XcmConfig, ExistentialDeposit, PriceForDelivery>(
	core::marker::PhantomData<(XcmConfig, ExistentialDeposit, PriceForDelivery)>,
);

#[cfg(feature = "runtime-benchmarks")]
impl<
		XcmConfig: xcm_executor::Config,
		ExistentialDeposit: Get<Option<Asset>>,
		PriceForDelivery: PriceForMessageDelivery<Id = ()>,
	> xcm_builder::EnsureDelivery
	for ToParentDeliveryHelper<XcmConfig, ExistentialDeposit, PriceForDelivery>
{
	fn ensure_successful_delivery(
		origin_ref: &Location,
		dest: &Location,
		fee_reason: xcm_executor::traits::FeeReason,
	) -> (Option<xcm_executor::FeesMode>, Option<Assets>) {
		use xcm::{latest::MAX_ITEMS_IN_ASSETS, MAX_INSTRUCTIONS_TO_DECODE};
		use xcm_executor::{traits::FeeManager, FeesMode};

		// check if the destination is relay/parent
		if dest.ne(&Location::parent()) {
			return (None, None);
		}

		let mut fees_mode = None;
		if !XcmConfig::FeeManager::is_waived(Some(origin_ref), fee_reason) {
			// if not waived, we need to set up accounts for paying and receiving fees

			// mint ED to origin if needed
			if let Some(ed) = ExistentialDeposit::get() {
				XcmConfig::AssetTransactor::deposit_asset(&ed, &origin_ref, None).unwrap();
			}

			// overestimate delivery fee
			let mut max_assets: Vec<Asset> = Vec::new();
			for i in 0..MAX_ITEMS_IN_ASSETS {
				max_assets.push((GeneralIndex(i as u128), 100u128).into());
			}
			let overestimated_xcm =
				vec![WithdrawAsset(max_assets.into()); MAX_INSTRUCTIONS_TO_DECODE as usize].into();
			let overestimated_fees = PriceForDelivery::price_for_delivery((), &overestimated_xcm);

			// mint overestimated fee to origin
			for fee in overestimated_fees.inner() {
				XcmConfig::AssetTransactor::deposit_asset(&fee, &origin_ref, None).unwrap();
			}

			// expected worst case - direct withdraw
			fees_mode = Some(FeesMode { jit_withdraw: true });
		}
		(fees_mode, None)
	}
}
