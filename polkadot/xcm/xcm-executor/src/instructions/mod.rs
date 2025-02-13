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

use crate::{config, XcmExecutor};
use xcm::{
	apply_instructions,
	latest::{Error as XcmError, InstructionsV6},
};

mod assets;
mod controls;
mod expect;
mod fees;
mod misc;
mod notifications;
mod origin;
mod query;
mod report;
mod versions;

pub trait ExecuteInstruction<Config: config::Config> {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError>;
}

macro_rules! impl_execute_instruction {
	($name:ident, $( $instr:ident $( < $instr_generic:ty > )? ),*) => {
		impl<Config: config::Config> ExecuteInstruction<Config> for $name<<Config as config::Config>::RuntimeCall> {
			fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
				match InstructionsV6::from(self) {
					$(
						Self::$instr(x) => x.execute(executor),
					)*
				}
			}
		}
	};
}

apply_instructions!(impl_execute_instruction, InstructionsV6);
