use core::marker::PhantomData;
use frame_support::{log, weights::Weight};
use xcm::latest::prelude::*;
use xcm_executor::traits::ShouldExecute;

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
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		max_weight: Weight,
		weight_credit: &mut Weight,
	) -> Result<(), ()> {
		Deny::should_execute(origin, message, max_weight, weight_credit)?;
		Allow::should_execute(origin, message, max_weight, weight_credit)
	}
}

// See issue #5233
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		_max_weight: Weight,
		_weight_credit: &mut Weight,
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

		// allow reserve transfers to arrive from relay chain
		if matches!(origin, MultiLocation { parents: 1, interior: Here }) &&
			message.0.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			log::warn!(
				target: "xcm::barrier",
				"Unexpected ReserveAssetDeposited from the relay chain",
			);
		}
		// Permit everything else
		Ok(())
	}
}
