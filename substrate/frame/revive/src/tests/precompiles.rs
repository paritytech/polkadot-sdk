// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Precompiles added to the test runtime.

use crate::{
	exec::{ErrorOrigin, ExecError},
	precompiles::{AddressMatcher, Error, Ext, ExtWithInfo, Precompile, Token},
	Config, DispatchError, Origin, Weight, U256,
};
use alloc::vec::Vec;
use alloy_core::{
	sol,
	sol_types::{PanicKind, SolValue},
};
use codec::Decode;
use core::{marker::PhantomData, num::NonZero};
use frame_system::RawOrigin;
use sp_runtime::traits::Dispatchable;

sol! {
	interface IWithInfo {
		function dummy() external;
	}

	interface INoInfo {
		function identity(uint64 number) external returns (uint64);
		function reverts(string calldata error) external;
		function panics() external;
		function errors() external;
		function consumeMaxGas() external;
		function callRuntime(bytes memory call) external;
		function passData(uint32 inputLen) external;
		function returnData(uint32 returnLen) external returns (bytes);
	}
}

pub struct WithInfo<T>(PhantomData<T>);

impl<T: Config> Precompile for WithInfo<T> {
	type T = T;
	type Interface = IWithInfo::IWithInfoCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0xFF_FF).unwrap());
	const HAS_CONTRACT_INFO: bool = true;

	fn call_with_info(
		_address: &[u8; 20],
		_input: &Self::Interface,
		_env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		Ok(Vec::new())
	}
}

pub struct NoInfo<T>(PhantomData<T>);

impl<T: Config> Precompile for NoInfo<T> {
	type T = T;
	type Interface = INoInfo::INoInfoCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(0xEF_FF).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use INoInfo::INoInfoCalls;

		match input {
			INoInfoCalls::identity(INoInfo::identityCall { number }) => Ok(number.abi_encode()),
			INoInfoCalls::reverts(INoInfo::revertsCall { error }) =>
				Err(Error::Revert(error.as_str().into())),
			INoInfoCalls::panics(INoInfo::panicsCall {}) =>
				Err(Error::Panic(PanicKind::Assert.into())),
			INoInfoCalls::errors(INoInfo::errorsCall {}) =>
				Err(Error::Error(DispatchError::Other("precompile failed").into())),
			INoInfoCalls::consumeMaxGas(INoInfo::consumeMaxGasCall {}) => {
				env.gas_meter_mut().charge(MaxGasToken)?;
				Ok(Vec::new())
			},
			INoInfoCalls::callRuntime(INoInfo::callRuntimeCall { call }) => {
				let origin = env.caller();
				let frame_origin = match origin {
					Origin::Root => RawOrigin::Root.into(),
					Origin::Signed(account_id) => RawOrigin::Signed(account_id.clone()).into(),
				};

				let call = <T as Config>::RuntimeCall::decode(&mut &call[..]).unwrap();
				match call.dispatch(frame_origin) {
					Ok(_) => Ok(Vec::new()),
					Err(e) =>
						Err(Error::Error(ExecError { error: e.error, origin: ErrorOrigin::Caller })),
				}
			},
			INoInfoCalls::passData(INoInfo::passDataCall { inputLen }) => {
				env.call(
					Weight::MAX,
					U256::MAX,
					&env.address(),
					0.into(),
					vec![42; *inputLen as usize],
					true,
					false,
				)?;
				Ok(Vec::new())
			},
			INoInfoCalls::returnData(INoInfo::returnDataCall { returnLen }) =>
				Ok(vec![42; *returnLen as usize]),
		}
	}
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct MaxGasToken;

impl<T: Config> Token<T> for MaxGasToken {
	fn weight(&self) -> Weight {
		Weight::MAX
	}
}
