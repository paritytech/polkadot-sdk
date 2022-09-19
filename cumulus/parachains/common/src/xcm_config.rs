use crate::impls::AccountIdOf;
use core::marker::PhantomData;
use frame_support::{
	log,
	traits::{fungibles::Inspect, tokens::BalanceConversion},
	weights::{Weight, WeightToFee, WeightToFeePolynomial},
};
use sp_runtime::traits::Get;
use xcm::latest::{prelude::*, Weight as XCMWeight};
use xcm_executor::traits::{FilterAssetLocation, ShouldExecute};

//TODO: move DenyThenTry to polkadot's xcm module.
/// Deny executing the XCM if it matches any of the Deny filter regardless of anything else.
/// If it passes the Deny, and matches one of the Allow cases then it is let through.
pub struct DenyThenTry<Deny, Allow>(PhantomData<Deny>, PhantomData<Allow>)
where
	Deny: ShouldExecute,
	Allow: ShouldExecute;

impl<Deny, Allow> ShouldExecute for DenyThenTry<Deny, Allow>
where
	Deny: ShouldExecute,
	Allow: ShouldExecute,
{
	fn should_execute<RuntimeCall>(
		origin: &MultiLocation,
		message: &mut Xcm<RuntimeCall>,
		max_weight: XCMWeight,
		weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		Deny::should_execute(origin, message, max_weight, weight_credit)?;
		Allow::should_execute(origin, message, max_weight, weight_credit)
	}
}

// See issue #5233
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<RuntimeCall>(
		origin: &MultiLocation,
		message: &mut Xcm<RuntimeCall>,
		_max_weight: XCMWeight,
		_weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		if message.0.iter().any(|inst| {
			matches!(
				inst,
				InitiateReserveWithdraw {
					reserve: MultiLocation { parents: 1, interior: Here },
					..
				} | DepositReserveAsset { dest: MultiLocation { parents: 1, interior: Here }, .. } |
					TransferReserveAsset {
						dest: MultiLocation { parents: 1, interior: Here },
						..
					}
			)
		}) {
			return Err(()) // Deny
		}

		// An unexpected reserve transfer has arrived from the Relay Chain. Generally, `IsReserve`
		// should not allow this, but we just log it here.
		if matches!(origin, MultiLocation { parents: 1, interior: Here }) &&
			message.0.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			log::warn!(
				target: "xcm::barrier",
				"Unexpected ReserveAssetDeposited from the Relay Chain",
			);
		}
		// Permit everything else
		Ok(())
	}
}

/// A `ChargeFeeInFungibles` implementation that converts the output of
/// a given WeightToFee implementation an amount charged in
/// a particular assetId from pallet-assets
pub struct AssetFeeAsExistentialDepositMultiplier<Runtime, WeightToFee, BalanceConverter>(
	PhantomData<(Runtime, WeightToFee, BalanceConverter)>,
);
impl<CurrencyBalance, Runtime, WeightToFee, BalanceConverter>
	cumulus_primitives_utility::ChargeWeightInFungibles<
		AccountIdOf<Runtime>,
		pallet_assets::Pallet<Runtime>,
	> for AssetFeeAsExistentialDepositMultiplier<Runtime, WeightToFee, BalanceConverter>
where
	Runtime: pallet_assets::Config,
	WeightToFee: WeightToFeePolynomial<Balance = CurrencyBalance>,
	BalanceConverter: BalanceConversion<
		CurrencyBalance,
		<Runtime as pallet_assets::Config>::AssetId,
		<Runtime as pallet_assets::Config>::Balance,
	>,
	AccountIdOf<Runtime>:
		From<polkadot_primitives::v2::AccountId> + Into<polkadot_primitives::v2::AccountId>,
{
	fn charge_weight_in_fungibles(
		asset_id: <pallet_assets::Pallet<Runtime> as Inspect<AccountIdOf<Runtime>>>::AssetId,
		weight: Weight,
	) -> Result<<pallet_assets::Pallet<Runtime> as Inspect<AccountIdOf<Runtime>>>::Balance, XcmError>
	{
		let amount = WeightToFee::weight_to_fee(&weight);
		// If the amount gotten is not at least the ED, then make it be the ED of the asset
		// This is to avoid burning assets and decreasing the supply
		let asset_amount = BalanceConverter::to_asset_balance(amount, asset_id)
			.map_err(|_| XcmError::TooExpensive)?;
		Ok(asset_amount)
	}
}

/// Accepts an asset if it is a native asset from a particular `MultiLocation`.
pub struct ConcreteNativeAssetFrom<Location>(PhantomData<Location>);
impl<Location: Get<MultiLocation>> FilterAssetLocation for ConcreteNativeAssetFrom<Location> {
	fn filter_asset_location(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		log::trace!(target: "xcm::filter_asset_location",
			"ConcreteNativeAsset asset: {:?}, origin: {:?}, location: {:?}",
			asset, origin, Location::get());
		matches!(asset.id, Concrete(ref id) if id == origin && origin == &Location::get())
	}
}

/// A generic function to use for MultiAssetFilter implementations, currently used to differentiate
/// between reserve operations and the rest of them.
pub fn weigh_multi_assets_generic(
	filter: &MultiAssetFilter,
	weight: Weight,
	max_assets: u32,
) -> XCMWeight {
	let multiplier = match filter {
		MultiAssetFilter::Definite(assets) => assets.len() as u64,
		MultiAssetFilter::Wild(_) => max_assets as u64,
	};
	weight.saturating_mul(multiplier).ref_time()
}
