use xcm::v3::prelude::XcmResult;

/// Executes optional logic when a `HrmpNewChannelOpenRequest` XCM notification is received from the relay chain.
/// A chain could, for example, decide to accept it or reject the request based on its own business logic,
/// and send a response back to the relay chain to open/close the channel.
pub trait HandleHrmpNewChannelOpenRequest {
	fn handle(sender: u32, max_message_size: u32, max_capacity: u32) -> XcmResult;
}

/// Executes optional logic when a `HrmpChannelAccepted` XCM notification is received from the relay chain.
/// If chain `sender` receives this notification, it means that chain `recipient`
/// has accepted the channel `sender` -> `recipient`.
/// The sender chain could, for example, decide to accept the other channel `recipient` -> `sender`,
/// once their request was accepted, by automatically sending a `Transact` message to the relay.
pub trait HandleHrmpChannelAccepted {
	fn handle(recipient: u32) -> XcmResult;
}

/// Executes optional logic when a `HrmpChannelClosing` XCM notification is received from the relay chain.
/// Both `sender` and `recipient` can close the channel, and the opposite party will be notified.
/// The chain could, for example, decide to close the other direction channel once this notification is received.
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
