// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use core::marker::PhantomData;
use snowbridge_core::operating_mode::ExportPausedQuery;
use sp_std::vec::Vec;
use xcm::{
	prelude::{Instruction, Location, SendError, SendResult, SendXcm, Xcm, XcmHash},
	VersionedLocation, VersionedXcm,
};
use xcm_builder::InspectMessageQueues;

pub struct PausableExporter<PausedQuery, InnerExporter>(PhantomData<(PausedQuery, InnerExporter)>);

impl<PausedQuery: ExportPausedQuery, InnerExporter: SendXcm> SendXcm
	for PausableExporter<PausedQuery, InnerExporter>
{
	type Ticket = InnerExporter::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		match PausedQuery::is_paused() {
			true => Err(SendError::NotApplicable),
			false => InnerExporter::validate(destination, message),
		}
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		match PausedQuery::is_paused() {
			true => Err(SendError::NotApplicable),
			false => InnerExporter::deliver(ticket),
		}
	}
}

impl<Halted: ExportPausedQuery, InnerExporter: SendXcm> InspectMessageQueues
	for PausableExporter<Halted, InnerExporter>
{
	fn clear_messages() {}

	/// This router needs to implement `InspectMessageQueues` but doesn't have to
	/// return any messages, since it just reuses the inner router.
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		Vec::new()
	}
}

/// Reject XCM messages whose first instruction is `DescendOrigin`.
///
/// `pallet_xcm::send` with a signed origin always prepends `DescendOrigin(AccountId32{..})`
/// before handing the message to the router. Legitimate bridge traffic (from `transfer_assets`
/// and similar) is executed locally and never produces a leading `DescendOrigin` in the outbound
/// XCM. Filtering these out on the source chain avoids forwarding clearly-invalid messages to
/// Bridge Hub over XCMP.
pub struct RejectDescendOriginExporter<InnerExporter>(PhantomData<InnerExporter>);

impl<InnerExporter: SendXcm> SendXcm for RejectDescendOriginExporter<InnerExporter> {
	type Ticket = InnerExporter::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		if let Some(Instruction::DescendOrigin(..)) =
			message.as_ref().and_then(|m| m.0.first())
		{
			return Err(SendError::NotApplicable);
		}
		InnerExporter::validate(destination, message)
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		InnerExporter::deliver(ticket)
	}
}

impl<InnerExporter: SendXcm> InspectMessageQueues
	for RejectDescendOriginExporter<InnerExporter>
{
	fn clear_messages() {}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		Vec::new()
	}
}
