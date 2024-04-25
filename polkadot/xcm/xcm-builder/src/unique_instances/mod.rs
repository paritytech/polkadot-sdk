use frame_support::traits::tokens::asset_ops::{
	common_asset_kinds::Instance, common_strategies::FromTo, Transfer,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::{ConvertLocation, Error as MatchError, MatchesInstance};

const LOG_TARGET: &str = "xcm::unique_instances";

pub mod backed_derivative;
pub mod recreateable;
pub mod transferable;

pub use backed_derivative::*;
pub use recreateable::*;
pub use transferable::*;

fn transfer_instance<
	AccountId,
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesInstance<InstanceTransfer::Id>,
	InstanceTransfer: for<'a> Transfer<Instance, FromTo<'a, AccountId>>,
>(
	what: &Asset,
	from: &Location,
	to: &Location,
) -> XcmResult {
	let instance_id = Matcher::matches_instance(what)?;
	let from =
		AccountIdConverter::convert_location(from).ok_or(MatchError::AccountIdConversionFailed)?;
	let to =
		AccountIdConverter::convert_location(to).ok_or(MatchError::AccountIdConversionFailed)?;

	InstanceTransfer::transfer(&instance_id, FromTo(&from, &to))
		.map_err(|e| XcmError::FailedToTransactAsset(e.into()))
}
