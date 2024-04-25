use super::LOG_TARGET;
use core::marker::PhantomData;
use frame_support::traits::{
	tokens::asset_ops::{
		common_asset_kinds::{Class, Instance},
		common_strategies::{DeriveIdFrom, FromTo, Owned},
		AssetDefinition, Create, Transfer,
	},
	Get,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, Error as MatchError, MatchesInstance, TransactAsset};

/// The status of a derivative instance.
pub enum DerivativeStatus<ClassId, InstanceId> {
	/// The derivative can be deposited (created) in the given class.
	DepositableIn(ClassId),

	/// The derivative already exists and it has the given ID.
	Exists(InstanceId),
}

/// The `BackedDerivativeInstanceAdapter` implements the `TransactAsset` for unique instances
/// (NFT-like entities).
///
/// The adapter uses the following asset operations:
/// * [`Create`] with the [`Owned`] strategy that uses the [`DeriveIdFrom`] id assignment.
///     * The [`DeriveIdFrom`] accepts the value of the `Id` type retrieved from the class [asset
///       definition](AssetDefinition)
///     (`ClassDef` generic parameter, represents a class-like entity such as a collection of NFTs).
/// * [`Transfer`] with [`FromTo`] strategy
///
/// This adapter assumes that a new asset can be created and an existing asset can be transferred.
/// Also, the adapter assumes that the asset can't be destroyed.
/// So, it transfers the asset to the `StashLocation` on withdrawal.
///
/// On deposit, the adapter consults the [`DerivativeStatus`] returned from the `Matcher`.
/// If the asset is depositable in a certain class, it will be created within that class.
/// Otherwise, if the asset exists, it will be transferred from the `StashLocation` to the
/// beneficiary.
///
/// Transfers work as expected, transferring the asset from the `from` location to the beneficiary.
///
/// This adapter is meant to be used in non-reserve locations where derivatives
/// can't be properly destroyed and then recreated.
///
/// For instance, an NFT engine on the chain (a pallet or a smart contract)
/// can be incapable of recreating an NFT with the same ID.
/// So, we can only create a derivative with a new ID.
/// In this context, if we burn a derivative on withdrawal:
/// 1. we could exhaust the ID space for derivatives
/// 2. if "burning" means transferring to a special address (like the zero address in EVM),
/// we also waste the active storage space for burned derivatives
///
/// To avoid that situation, the `StashLocation` is used to hold the withdrawn derivatives.
///
/// Also, this adapter can be used in the NFT engine that simply doesn't support burning NFTs.
pub struct BackedDerivativeInstanceAdapter<
	AccountId,
	AccountIdConverter,
	Matcher,
	ClassDef,
	InstanceOps,
	StashLocation,
>(PhantomData<(AccountId, AccountIdConverter, Matcher, ClassDef, InstanceOps, StashLocation)>);

impl<AccountId, AccountIdConverter, Matcher, ClassDef, InstanceOps, StashLocation> TransactAsset
	for BackedDerivativeInstanceAdapter<
		AccountId,
		AccountIdConverter,
		Matcher,
		ClassDef,
		InstanceOps,
		StashLocation,
	> where
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesInstance<DerivativeStatus<ClassDef::Id, InstanceOps::Id>>,
	ClassDef: AssetDefinition<Class>,
	for<'a> InstanceOps: AssetDefinition<Instance>
		+ Create<Instance, Owned<'a, DeriveIdFrom<'a, ClassDef::Id, InstanceOps::Id>, AccountId>>
		+ Transfer<Instance, FromTo<'a, AccountId>>,
	StashLocation: Get<Location>,
{
	fn deposit_asset(what: &Asset, who: &Location, context: Option<&XcmContext>) -> XcmResult {
		log::trace!(
			target: LOG_TARGET,
			"BackedDerivativeInstanceAdapter::deposit_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			context,
		);

		let derivative_status = Matcher::matches_instance(what)?;
		let to = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		let result = match derivative_status {
			DerivativeStatus::DepositableIn(class_id) =>
				InstanceOps::create(Owned::new(DeriveIdFrom::parent_id(&class_id), &to))
					.map(|_id| ()),
			DerivativeStatus::Exists(instance_id) => {
				let from = AccountIdConverter::convert_location(&StashLocation::get())
					.ok_or(MatchError::AccountIdConversionFailed)?;

				InstanceOps::transfer(&instance_id, FromTo(&from, &to))
			},
		};

		result.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"BackedDerivativeInstanceAdapter::withdraw_asset what: {:?}, who: {:?}, context: {:?}",
			what,
			who,
			maybe_context,
		);

		let derivative_status = Matcher::matches_instance(what)?;
		let from = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		if let DerivativeStatus::Exists(instance_id) = derivative_status {
			let to = AccountIdConverter::convert_location(&StashLocation::get())
				.ok_or(MatchError::AccountIdConversionFailed)?;

			InstanceOps::transfer(&instance_id, FromTo(&from, &to))
				.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

			Ok(what.clone().into())
		} else {
			Err(XcmError::NotWithdrawable)
		}
	}

	fn internal_transfer_asset(
		what: &Asset,
		from: &Location,
		to: &Location,
		context: &XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		log::trace!(
			target: LOG_TARGET,
			"BackedDerivativeInstanceAdapter::internal_transfer_asset what: {:?}, from: {:?}, to: {:?}, context: {:?}",
			what,
			from,
			to,
			context,
		);

		let derivative_status = Matcher::matches_instance(what)?;
		let from = AccountIdConverter::convert_location(from)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let to = AccountIdConverter::convert_location(to)
			.ok_or(MatchError::AccountIdConversionFailed)?;

		if let DerivativeStatus::Exists(instance_id) = derivative_status {
			InstanceOps::transfer(&instance_id, FromTo(&from, &to))
				.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

			Ok(what.clone().into())
		} else {
			Err(XcmError::NotWithdrawable)
		}
	}
}
