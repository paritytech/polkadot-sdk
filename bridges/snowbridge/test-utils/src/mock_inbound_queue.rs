// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use crate::FAILING_NONCE;
use snowbridge_core::reward::{AddTip, AddTipError};

pub struct MockOkInboundQueue;

impl AddTip for MockOkInboundQueue {
	fn add_tip(nonce: u64, _amount: u128) -> Result<(), AddTipError> {
		// Force an error condition
		if nonce == FAILING_NONCE {
			return Err(AddTipError::NonceConsumed)
		}
		Ok(())
	}
}
