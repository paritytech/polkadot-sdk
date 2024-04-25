use super::{transfer_instance, LOG_TARGET};
use core::marker::PhantomData;
use frame_support::traits::{
	tokens::asset_ops::{common_asset_kinds::Instance, common_strategies::FromTo, Transfer},
	Get,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, MatchesInstance, TransactAsset};

/// The `TransferableInstanceAdapter` implements the `TransactAsset` for unique instances (NFT-like
/// entities).
///
/// The adapter uses only the [`Transfer`] asset operation with the [`FromTo`] strategy.
///
/// It is meant to be used when the asset can't be safely destroyed on withdrawal
/// (i.e., the absence of the loss of important data can't be guaranteed when the asset is
/// destroyed). Equivalently, this adapter may be used when the asset can't be recreated on deposit.
///
/// The adapter uses the `StashLocation` as the beneficiary to transfer the asset on withdrawal.
/// On deposit, the asset will be transferred from the `StashLocation` to the beneficiary.
///
/// Transfers work as expected, transferring the asset from the `from` location to the beneficiary.
///
/// This adapter can be used only in a reserve location.
/// It can't create new instances, hence it can't create derivatives.
pub struct TransferableInstanceAdapter<
	AccountId,
	AccountIdConverter,
	Matcher,
	InstanceTransfer,
	StashLocation,
>(PhantomData<(AccountId, AccountIdConverter, Matcher, InstanceTransfer, StashLocation)>);

impl<
		AccountId,
		AccountIdConverter: ConvertLocation<AccountId>,
		Matcher: MatchesInstance<InstanceTransfer::Id>,
		InstanceTransfer: for<'a> Transfer<Instance, FromTo<'a, AccountId>>,
		StashLocation: Get<Location>,
	> TransactAsset
	for TransferableInstanceAdapter<
		AccountId,
		AccountIdConverter,
		Matcher,
		InstanceTransfer,
		StashLocation,
	>
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"TransferableInstanceAdapter::deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);

		transfer_instance::<AccountId, AccountIdConverter, Matcher, InstanceTransfer>(
			what,
			&StashLocation::get(),
			who,
		)
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"TransferableInstanceAdapter::withdraw_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			maybe_context,
		);

		transfer_instance::<AccountId, AccountIdConverter, Matcher, InstanceTransfer>(
			what,
			who,
			&StashLocation::get(),
		)?;

		Ok(what.clone().into())
	}

	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"TransferableInstanceAdapter::internal_transfer_asset what: {:?}, from: {:?}, to: {:?}, context: {:?}",
			what,
			from,
			to,
			context,
		);

		transfer_instance::<AccountId, AccountIdConverter, Matcher, InstanceTransfer>(
			what, from, to,
		)?;

		Ok(what.clone().into())
	}
}
