use xcm::v3::{
	prelude::{XcmError, XcmResult},
};

pub trait HandleHrmp {
	fn handle_new_channel_open_request(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult;
	fn handle_channel_accepted(recipient: u32) -> XcmResult;
	fn handle_channel_closing(initiator: u32, sender: u32, recipient: u32) -> XcmResult;
}

impl HandleHrmp for () {
	fn handle_new_channel_open_request(_sender: u32, _max_message_size: u32, _max_capacity: u32) -> XcmResult {
		Err(XcmError::Unimplemented)
	}

	fn handle_channel_accepted(_recipient: u32) -> XcmResult {
		Err(XcmError::Unimplemented)
	}

	fn handle_channel_closing(_initiator: u32, _sender: u32, _recipient: u32) -> XcmResult {
		Err(XcmError::Unimplemented)
	}
}
