// Copyright (C) Parity Technologies (UK) Ltd.
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
	defensive,
	traits::{tokens::fungibles, Get, OnUnbalanced as OnUnbalancedT},
	weights::{Weight, WeightToFee as WeightToFeeT},
	CloneNoBound,
};
use pallet_asset_conversion::SwapCredit as SwapCreditT;
use polkadot_runtime_common::xcm_sender::PriceForMessageDelivery;
use sp_runtime::{
	traits::{Saturating, Zero},
	SaturatedConversion,
};
use sp_std::{marker::PhantomData, prelude::*};
use xcm::{latest::prelude::*, WrapVersion};
use xcm_builder::TakeRevenue;
use xcm_executor::{
	traits::{MatchesFungibles, TransactAsset, WeightTrader},
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
	outstanding_concrete_asset: Asset,
}

/// Charges for execution in the first asset of those selected for fee payment
/// Only succeeds for Concrete Fungible Assets
/// First tries to convert the this Asset into a local assetId
/// Then charges for this assetId as described by FeeCharger
/// Weight, paid balance, local asset Id and the location is stored for
/// later refund purposes
/// Important: Errors if the Trader is being called twice by 2 BuyExecution instructions
/// Alternatively we could just return payment in the aforementioned case
#[derive(CloneNoBound)]
pub struct TakeFirstAssetTrader<
	AccountId: Eq,
	FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
	Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
	ConcreteAssets: fungibles::Mutate<AccountId> + fungibles::Balanced<AccountId>,
	HandleRefund: TakeRevenue,
>(
	Option<AssetTraderRefunder>,
	PhantomData<(AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund)>,
);
impl<
		AccountId: Eq,
		FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
		Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
		ConcreteAssets: fungibles::Mutate<AccountId> + fungibles::Balanced<AccountId>,
		HandleRefund: TakeRevenue,
	> WeightTrader
	for TakeFirstAssetTrader<AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund>
{
	fn new() -> Self {
		Self(None, PhantomData)
	}
	// We take first asset
	// Check whether we can convert fee to asset_fee (is_sufficient, min_deposit)
	// If everything goes well, we charge.
	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: xcm_executor::AssetsInHolding,
		context: &XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(target: "xcm::weight", "TakeFirstAssetTrader::buy_weight weight: {:?}, payment: {:?}, context: {:?}", weight, payment, context);

		// Make sure we don't enter twice
		if self.0.is_some() {
			return Err(XcmError::NotWithdrawable)
		}

		// We take the very first asset from payment
		// (assets are sorted by fungibility/amount after this conversion)
		let assets: Assets = payment.clone().into();

		// Take the first asset from the selected Assets
		let first = assets.get(0).ok_or(XcmError::AssetNotFound)?;

		// Get the local asset id in which we can pay for fees
		let (local_asset_id, _) =
			Matcher::matches_fungibles(first).map_err(|_| XcmError::AssetNotFound)?;

		// Calculate how much we should charge in the asset_id for such amount of weight
		// Require at least a payment of minimum_balance
		// Necessary for fully collateral-backed assets
		let asset_balance: u128 =
			FeeCharger::charge_weight_in_fungibles(local_asset_id.clone(), weight)
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

		// Convert to the same kind of asset, with the required fungible balance
		let required = first.id.clone().into_asset(asset_balance.into());

		// Subtract payment
		let unused = payment.checked_sub(required.clone()).map_err(|_| XcmError::TooExpensive)?;

		// record weight and asset
		self.0 = Some(AssetTraderRefunder {
			weight_outstanding: weight,
			outstanding_concrete_asset: required,
		});

		Ok(unused)
	}

	fn refund_weight(&mut self, weight: Weight, context: &XcmContext) -> Option<Asset> {
		log::trace!(target: "xcm::weight", "TakeFirstAssetTrader::refund_weight weight: {:?}, context: {:?}", weight, context);
		if let Some(AssetTraderRefunder {
			mut weight_outstanding,
			outstanding_concrete_asset: Asset { id, fun },
		}) = self.0.clone()
		{
			// Get the local asset id in which we can refund fees
			let (local_asset_id, outstanding_balance) =
				Matcher::matches_fungibles(&(id.clone(), fun).into()).ok()?;

			let minimum_balance = ConcreteAssets::minimum_balance(local_asset_id.clone());

			// Calculate asset_balance
			// This read should have already be cached in buy_weight
			let (asset_balance, outstanding_minus_subtracted) =
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
			let outstanding_minus_subtracted: u128 = outstanding_minus_subtracted.saturated_into();
			let asset_balance: u128 = asset_balance.saturated_into();

			// Construct outstanding_concrete_asset with the same location id and subtracted
			// balance
			let outstanding_concrete_asset: Asset =
				(id.clone(), outstanding_minus_subtracted).into();

			// Subtract from existing weight and balance
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
		AccountId: Eq,
		FeeCharger: ChargeWeightInFungibles<AccountId, ConcreteAssets>,
		Matcher: MatchesFungibles<ConcreteAssets::AssetId, ConcreteAssets::Balance>,
		ConcreteAssets: fungibles::Mutate<AccountId> + fungibles::Balanced<AccountId>,
		HandleRefund: TakeRevenue,
	> Drop for TakeFirstAssetTrader<AccountId, FeeCharger, Matcher, ConcreteAssets, HandleRefund>
{
	fn drop(&mut self) {
		if let Some(asset_trader) = self.0.clone() {
			HandleRefund::take_revenue(asset_trader.outstanding_concrete_asset);
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
pub struct SwapFirstAssetTrader<
	Target: Get<Fungibles::AssetId>,
	SwapCredit: SwapCreditT<
		AccountId,
		Balance = Fungibles::Balance,
		AssetKind = Fungibles::AssetId,
		Credit = fungibles::Credit<AccountId, Fungibles>,
	>,
	WeightToFee: WeightToFeeT<Balance = Fungibles::Balance>,
	Fungibles: fungibles::Balanced<AccountId>,
	FungiblesAssetMatcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
	OnUnbalanced: OnUnbalancedT<fungibles::Credit<AccountId, Fungibles>>,
	AccountId,
> where
	Fungibles::Balance: Into<u128>,
{
	/// Accumulated fee paid for XCM execution.
	total_fee: fungibles::Credit<AccountId, Fungibles>,
	/// Last asset utilized by a client to settle a fee.
	last_fee_asset: Option<AssetId>,
	_phantom_data: PhantomData<(
		Target,
		SwapCredit,
		WeightToFee,
		Fungibles,
		FungiblesAssetMatcher,
		OnUnbalanced,
		AccountId,
	)>,
}

impl<
		Target: Get<Fungibles::AssetId>,
		SwapCredit: SwapCreditT<
			AccountId,
			Balance = Fungibles::Balance,
			AssetKind = Fungibles::AssetId,
			Credit = fungibles::Credit<AccountId, Fungibles>,
		>,
		WeightToFee: WeightToFeeT<Balance = Fungibles::Balance>,
		Fungibles: fungibles::Balanced<AccountId>,
		FungiblesAssetMatcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
		OnUnbalanced: OnUnbalancedT<fungibles::Credit<AccountId, Fungibles>>,
		AccountId,
	> WeightTrader
	for SwapFirstAssetTrader<
		Target,
		SwapCredit,
		WeightToFee,
		Fungibles,
		FungiblesAssetMatcher,
		OnUnbalanced,
		AccountId,
	> where
	Fungibles::Balance: Into<u128>,
{
	fn new() -> Self {
		Self {
			total_fee: fungibles::Credit::<AccountId, Fungibles>::zero(Target::get()),
			last_fee_asset: None,
			_phantom_data: PhantomData,
		}
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		mut payment: AssetsInHolding,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		log::trace!(
			target: "xcm::weight",
			"SwapFirstAssetTrader::buy_weight weight: {:?}, payment: {:?}",
			weight,
			payment,
		);
		let first_asset: Asset =
			payment.fungible.pop_first().ok_or(XcmError::AssetNotFound)?.into();
		let (fungibles_asset, balance) = FungiblesAssetMatcher::matches_fungibles(&first_asset)
			.map_err(|_| XcmError::AssetNotFound)?;

		let swap_asset = fungibles_asset.clone().into();
		if Target::get().eq(&swap_asset) {
			// current trader is not applicable.
			return Err(XcmError::FeesNotMet)
		}

		let credit_in = Fungibles::issue(fungibles_asset, balance);
		let fee = WeightToFee::weight_to_fee(&weight);

		// swap the user's asset for the `Target` asset.
		let (credit_out, credit_change) = SwapCredit::swap_tokens_for_exact_tokens(
			vec![swap_asset, Target::get()],
			credit_in,
			fee,
		)
		.map_err(|(credit_in, _)| {
			drop(credit_in);
			XcmError::FeesNotMet
		})?;

		match self.total_fee.subsume(credit_out) {
			Err(credit_out) => {
				// error may occur if `total_fee.asset` differs from `credit_out.asset`, which does
				// not apply in this context.
				defensive!(
					"`total_fee.asset` must be equal to `credit_out.asset`",
					(self.total_fee.asset(), credit_out.asset())
				);
				return Err(XcmError::FeesNotMet)
			},
			_ => (),
		};
		self.last_fee_asset = Some(first_asset.id.clone());

		payment.fungible.insert(first_asset.id, credit_change.peek().into());
		drop(credit_change);
		Ok(payment)
	}

	fn refund_weight(&mut self, weight: Weight, _context: &XcmContext) -> Option<Asset> {
		log::trace!(
			target: "xcm::weight",
			"SwapFirstAssetTrader::refund_weight weight: {:?}, self.total_fee: {:?}",
			weight,
			self.total_fee,
		);
		if self.total_fee.peek().is_zero() {
			// noting yet paid to refund.
			return None
		}
		let mut refund_asset = if let Some(asset) = &self.last_fee_asset {
			// create an initial zero refund in the asset used in the last `buy_weight`.
			(asset.clone(), Fungible(0)).into()
		} else {
			return None
		};
		let refund_amount = WeightToFee::weight_to_fee(&weight);
		if refund_amount >= self.total_fee.peek() {
			// not enough was paid to refund the `weight`.
			return None
		}

		let refund_swap_asset = FungiblesAssetMatcher::matches_fungibles(&refund_asset)
			.map(|(a, _)| a.into())
			.ok()?;

		let refund = self.total_fee.extract(refund_amount);
		let refund = match SwapCredit::swap_exact_tokens_for_tokens(
			vec![Target::get(), refund_swap_asset],
			refund,
			None,
		) {
			Ok(refund_in_target) => refund_in_target,
			Err((refund, _)) => {
				// return an attempted refund back to the `total_fee`.
				let _ = self.total_fee.subsume(refund).map_err(|refund| {
					// error may occur if `total_fee.asset` differs from `refund.asset`, which does
					// not apply in this context.
					defensive!(
						"`total_fee.asset` must be equal to `refund.asset`",
						(self.total_fee.asset(), refund.asset())
					);
				});
				return None
			},
		};

		refund_asset.fun = refund.peek().into().into();
		drop(refund);
		Some(refund_asset)
	}
}

impl<
		Target: Get<Fungibles::AssetId>,
		SwapCredit: SwapCreditT<
			AccountId,
			Balance = Fungibles::Balance,
			AssetKind = Fungibles::AssetId,
			Credit = fungibles::Credit<AccountId, Fungibles>,
		>,
		WeightToFee: WeightToFeeT<Balance = Fungibles::Balance>,
		Fungibles: fungibles::Balanced<AccountId>,
		FungiblesAssetMatcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
		OnUnbalanced: OnUnbalancedT<fungibles::Credit<AccountId, Fungibles>>,
		AccountId,
	> Drop
	for SwapFirstAssetTrader<
		Target,
		SwapCredit,
		WeightToFee,
		Fungibles,
		FungiblesAssetMatcher,
		OnUnbalanced,
		AccountId,
	> where
	Fungibles::Balance: Into<u128>,
{
	fn drop(&mut self) {
		if self.total_fee.peek().is_zero() {
			return
		}
		let total_fee = self.total_fee.extract(self.total_fee.peek());
		OnUnbalanced::on_unbalanced(total_fee);
	}
}

#[cfg(test)]
mod test_xcm_router {
	use super::*;
	use cumulus_primitives_core::UpwardMessage;

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
}
#[cfg(test)]
mod test_trader {
	use super::*;
	use frame_support::{
		assert_ok,
		traits::tokens::{
			DepositConsequence, Fortitude, Preservation, Provenance, WithdrawConsequence,
		},
	};
	use sp_runtime::DispatchError;
	use xcm_executor::{traits::Error, AssetsInHolding};

	#[test]
	fn take_first_asset_trader_buy_weight_called_twice_throws_error() {
		const AMOUNT: u128 = 100;

		// prepare prerequisites to instantiate `TakeFirstAssetTrader`
		type TestAccountId = u32;
		type TestAssetId = u32;
		type TestBalance = u128;
		struct TestAssets;
		impl MatchesFungibles<TestAssetId, TestBalance> for TestAssets {
			fn matches_fungibles(a: &Asset) -> Result<(TestAssetId, TestBalance), Error> {
				match a {
					Asset { fun: Fungible(amount), id: AssetId(_id) } => Ok((1, *amount)),
					_ => Err(Error::AssetNotHandled),
				}
			}
		}
		impl fungibles::Inspect<TestAccountId> for TestAssets {
			type AssetId = TestAssetId;
			type Balance = TestBalance;

			fn total_issuance(_: Self::AssetId) -> Self::Balance {
				todo!()
			}

			fn minimum_balance(_: Self::AssetId) -> Self::Balance {
				0
			}

			fn balance(_: Self::AssetId, _: &TestAccountId) -> Self::Balance {
				todo!()
			}

			fn total_balance(_: Self::AssetId, _: &TestAccountId) -> Self::Balance {
				todo!()
			}

			fn reducible_balance(
				_: Self::AssetId,
				_: &TestAccountId,
				_: Preservation,
				_: Fortitude,
			) -> Self::Balance {
				todo!()
			}

			fn can_deposit(
				_: Self::AssetId,
				_: &TestAccountId,
				_: Self::Balance,
				_: Provenance,
			) -> DepositConsequence {
				todo!()
			}

			fn can_withdraw(
				_: Self::AssetId,
				_: &TestAccountId,
				_: Self::Balance,
			) -> WithdrawConsequence<Self::Balance> {
				todo!()
			}

			fn asset_exists(_: Self::AssetId) -> bool {
				todo!()
			}
		}
		impl fungibles::Mutate<TestAccountId> for TestAssets {}
		impl fungibles::Balanced<TestAccountId> for TestAssets {
			type OnDropCredit = fungibles::DecreaseIssuance<TestAccountId, Self>;
			type OnDropDebt = fungibles::IncreaseIssuance<TestAccountId, Self>;
		}
		impl fungibles::Unbalanced<TestAccountId> for TestAssets {
			fn handle_dust(_: fungibles::Dust<TestAccountId, Self>) {
				todo!()
			}
			fn write_balance(
				_: Self::AssetId,
				_: &TestAccountId,
				_: Self::Balance,
			) -> Result<Option<Self::Balance>, DispatchError> {
				todo!()
			}

			fn set_total_issuance(_: Self::AssetId, _: Self::Balance) {
				todo!()
			}
		}

		struct FeeChargerAssetsHandleRefund;
		impl ChargeWeightInFungibles<TestAccountId, TestAssets> for FeeChargerAssetsHandleRefund {
			fn charge_weight_in_fungibles(
				_: <TestAssets as fungibles::Inspect<TestAccountId>>::AssetId,
				_: Weight,
			) -> Result<<TestAssets as fungibles::Inspect<TestAccountId>>::Balance, XcmError> {
				Ok(AMOUNT)
			}
		}
		impl TakeRevenue for FeeChargerAssetsHandleRefund {
			fn take_revenue(_: Asset) {}
		}

		// create new instance
		type Trader = TakeFirstAssetTrader<
			TestAccountId,
			FeeChargerAssetsHandleRefund,
			TestAssets,
			TestAssets,
			FeeChargerAssetsHandleRefund,
		>;
		let mut trader = <Trader as WeightTrader>::new();
		let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

		// prepare test data
		let asset: Asset = (Here, AMOUNT).into();
		let payment = AssetsInHolding::from(asset);
		let weight_to_buy = Weight::from_parts(1_000, 1_000);

		// lets do first call (success)
		assert_ok!(trader.buy_weight(weight_to_buy, payment.clone(), &ctx));

		// lets do second call (error)
		assert_eq!(trader.buy_weight(weight_to_buy, payment, &ctx), Err(XcmError::NotWithdrawable));
	}
}

/// Implementation of `xcm_builder::EnsureDelivery` which helps to ensure delivery to the
/// parent relay chain. Deposits existential deposit for origin (if needed).
/// Deposits estimated fee to the origin account (if needed).
/// Allows triggering of additional logic for a specific `ParaId` (e.g. to open an HRMP channel) if
/// needed.
#[cfg(feature = "runtime-benchmarks")]
pub struct ToParentDeliveryHelper<XcmConfig, ExistentialDeposit, PriceForDelivery>(
	sp_std::marker::PhantomData<(XcmConfig, ExistentialDeposit, PriceForDelivery)>,
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
		use xcm::latest::{MAX_INSTRUCTIONS_TO_DECODE, MAX_ITEMS_IN_ASSETS};
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
