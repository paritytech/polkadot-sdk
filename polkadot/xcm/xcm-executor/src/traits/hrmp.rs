use xcm::v3::prelude::XcmResult;

pub trait HandleHrmpNewChannelOpenRequest {
	fn handle(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult;
}
pub trait HandleHrmpChannelAccepted {
	fn handle(recipient: u32) -> XcmResult;
}
pub trait HandleHrmpChannelClosing {
	fn handle(initiator: u32, sender: u32, recipient: u32) -> XcmResult;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpNewChannelOpenRequest for Tuple {
	fn handle(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(sender, max_message_size, max_capacity)?; )* );
		Ok(())
	}
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpChannelAccepted for Tuple {
	fn handle(recipient: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(recipient)?; )* );
		Ok(())
	}
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HandleHrmpChannelClosing for Tuple {
	fn handle(initiator: u32, sender: u32, recipient: u32) -> XcmResult {
		for_tuples!( #( Tuple::handle(initiator, sender, recipient)?; )* );
		Ok(())
	}
}
