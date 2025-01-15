// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::ExecuteInstruction;
use crate::{
	config,
	traits::{
		HandleHrmpChannelAccepted, HandleHrmpChannelClosing, HandleHrmpNewChannelOpenRequest,
		ProcessTransaction,
	},
	XcmExecutor,
};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;

impl<Config: config::Config> ExecuteInstruction<Config> for HrmpNewChannelOpenRequest {
	fn execute(self, _executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity } = self;
		Config::TransactionalProcessor::process(|| {
			Config::HrmpNewChannelOpenRequestHandler::handle(sender, max_message_size, max_capacity)
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for HrmpChannelAccepted {
	fn execute(self, _executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let HrmpChannelAccepted { recipient } = self;
		Config::TransactionalProcessor::process(|| {
			Config::HrmpChannelAcceptedHandler::handle(recipient)
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for HrmpChannelClosing {
	fn execute(self, _executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let HrmpChannelClosing { initiator, sender, recipient } = self;
		Config::TransactionalProcessor::process(|| {
			Config::HrmpChannelClosingHandler::handle(initiator, sender, recipient)
		})
	}
}
