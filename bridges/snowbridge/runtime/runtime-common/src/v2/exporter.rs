use core::marker::PhantomData;
use frame_support::traits::Get;
use sp_std::vec;
use xcm::prelude::{
	validate_send, ExportMessage, InteriorLocation, Location, SendError,
	SendError::{MissingArgument, NotApplicable},
	SendResult, SendXcm, SetTopic, Unlimited, UnpaidExecution, Xcm, XcmHash,
};
use xcm_builder::{ensure_is_remote, ExporterFor};

/// A custom UnpaidRemoteExporter in Snowbridge with a minor tweak allowes some payment to be
/// attached, which is not permitted by the original UnpaidRemoteExporter.
pub struct SnowbridgeUnpaidRemoteExporter<Bridges, Router, UniversalLocation>(
	PhantomData<(Bridges, Router, UniversalLocation)>,
);
impl<Bridges: ExporterFor, Router: SendXcm, UniversalLocation: Get<InteriorLocation>> SendXcm
	for SnowbridgeUnpaidRemoteExporter<Bridges, Router, UniversalLocation>
{
	type Ticket = Router::Ticket;

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Router::Ticket> {
		// This `clone` ensures that `dest` is not consumed in any case.
		let d = dest.clone().ok_or(MissingArgument)?;
		let devolved = ensure_is_remote(UniversalLocation::get(), d).map_err(|_| NotApplicable)?;
		let (remote_network, remote_location) = devolved;
		let xcm = msg.take().ok_or(MissingArgument)?;

		// find exporter
		let Some((bridge, maybe_payment)) =
			Bridges::exporter_for(&remote_network, &remote_location, &xcm)
		else {
			// We need to make sure that msg is not consumed in case of `NotApplicable`.
			*msg = Some(xcm);
			return Err(NotApplicable)
		};

		// `xcm` should already end with `SetTopic` - if it does, then extract and derive into
		// an onward topic ID.
		let maybe_forward_id = match xcm.last() {
			Some(SetTopic(t)) => Some(*t),
			_ => None,
		};

		// We then send a normal message to the bridge asking it to export the prepended
		// message to the remote chain. This will only work if the bridge will do the message
		// export for free. Common-good chains will typically be afforded this.
		let mut message = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			ExportMessage {
				network: remote_network,
				destination: remote_location,
				xcm: xcm.clone(),
			},
		]);
		if let Some(forward_id) = maybe_forward_id {
			message.0.push(SetTopic(forward_id));
		}
		let (v, mut cost) = validate_send::<Router>(bridge, message).inspect_err(|err| {
			if let NotApplicable = err {
				// We need to make sure that msg is not consumed in case of `NotApplicable`.
				*msg = Some(xcm);
			}
		})?;
		if let Some(bridge_payment) = maybe_payment {
			cost.push(bridge_payment);
		}
		Ok((v, cost))
	}

	fn deliver(validation: Self::Ticket) -> Result<XcmHash, SendError> {
		Router::deliver(validation)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_delivery(location: Option<Location>) {
		Router::ensure_successful_delivery(location);
	}
}
