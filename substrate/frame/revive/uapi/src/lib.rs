// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! External C API to communicate with substrate contracts runtime module.
//!
//! Refer to substrate FRAME contract module for more documentation.

#![no_std]

mod flags;
pub use flags::*;
mod host;

pub use host::{HostFn, HostFnImpl};

macro_rules! define_error_codes {
    (
        $(
            $( #[$attr:meta] )*
            $name:ident = $discr:literal,
        )*
    ) => {
        /// Every error that can be returned to a contract when it calls any of the host functions.
        #[derive(Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub enum ReturnErrorCode {
            /// API call successful.
            Success = 0,
            $(
                $( #[$attr] )*
                $name = $discr,
            )*
            /// Returns if an unknown error was received from the host module.
            Unknown,
        }

        impl From<ReturnCode> for Result {
            fn from(return_code: ReturnCode) -> Self {
                match return_code.0 {
                    0 => Ok(()),
                    $(
                        $discr => Err(ReturnErrorCode::$name),
                    )*
                    _ => Err(ReturnErrorCode::Unknown),
                }
            }
        }
    };
}

impl From<ReturnErrorCode> for u32 {
	fn from(code: ReturnErrorCode) -> u32 {
		code as u32
	}
}

impl From<ReturnErrorCode> for u64 {
	fn from(error: ReturnErrorCode) -> Self {
		u32::from(error).into()
	}
}

define_error_codes! {
	/// The called function trapped and has its state changes reverted.
	/// In this case no output buffer is returned.
	/// Can only be returned from `call` and `instantiate`.
	CalleeTrapped = 1,
	/// The called function ran to completion but decided to revert its state.
	/// An output buffer is returned when one was supplied.
	/// Can only be returned from `call` and `instantiate`.
	CalleeReverted = 2,
	/// The passed key does not exist in storage.
	KeyNotFound = 3,
	/// Transfer failed for other not further specified reason. Most probably
	/// reserved or locked balance of the sender that was preventing the transfer.
	TransferFailed = 4,
	/// No code could be found at the supplied code hash.
	CodeNotFound = 5,
	/// The account that was called is no contract.
	NotCallable = 6,
	/// The call to `debug_message` had no effect because debug message
	/// recording was disabled.
	LoggingDisabled = 7,
	/// The call dispatched by `call_runtime` was executed but returned an error.
	CallRuntimeFailed = 8,
	/// ECDSA public key recovery failed. Most probably wrong recovery id or signature.
	EcdsaRecoveryFailed = 9,
	/// sr25519 signature verification failed.
	Sr25519VerifyFailed = 10,
	/// The `xcm_execute` call failed.
	XcmExecutionFailed = 11,
	/// The `xcm_send` call failed.
	XcmSendFailed = 12,
}

/// The raw return code returned by the host side.
#[repr(transparent)]
pub struct ReturnCode(u32);

/// Used as a sentinel value when reading and writing contract memory.
///
/// We use this value to signal `None` to a contract when only a primitive is
/// allowed and we don't want to go through encoding a full Rust type.
/// Using `u32::Max` is a safe sentinel because contracts are never
/// allowed to use such a large amount of resources. So this value doesn't
/// make sense for a memory location or length.
const SENTINEL: u32 = u32::MAX;

impl From<ReturnCode> for Option<u32> {
	fn from(code: ReturnCode) -> Self {
		(code.0 < SENTINEL).then_some(code.0)
	}
}

impl ReturnCode {
	/// Returns the raw underlying `u32` representation.
	pub fn into_u32(self) -> u32 {
		self.0
	}
	/// Returns the underlying `u32` converted into `bool`.
	pub fn into_bool(self) -> bool {
		self.0.ne(&0)
	}
}

type Result = core::result::Result<(), ReturnErrorCode>;
