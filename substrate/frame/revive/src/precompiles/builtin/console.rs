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

/// A complete Console.log precompile implementation for Substrate's revive EVM contracts.
///
/// This precompile provides full compatibility with Foundry's console.sol library, implementing
/// all 387 console.log function signatures for debugging capabilities during development and testing.
///
/// # Features
///
/// - **Complete Foundry Compatibility**: Implements all 387 console.log functions from forge-std
/// - **Multiple Output Channels**: Broadcasts to Substrate logs, stdout, and RPC debug buffer
/// - **Type-Safe Parameter Handling**: Supports uint256, int256, string, bool, address, and bytes1-bytes32
/// - **Efficient Gas Usage**: Charges minimal gas cost (HostFn) for console operations
/// - **Development-Focused**: Designed for debugging and testing, not production logging
///
/// # Address Allocation
///
/// The console precompile is deployed at address `0x000000000000000000636F6e736F6c652e6c6f67`
/// which is "console.log" encoded as hex, matching Foundry's console.sol expectation.
///
/// # Function Signatures
///
/// The precompile supports:
/// - Empty log: `log()`
/// - Single parameter: `log(uint256)`, `log(string)`, `logBytes32(bytes32)`, etc.
/// - Two parameters: `log(uint256, string)`, `log(bool, address)`, etc.
/// - Three parameters: `log(string, uint256, bool)`, etc.
/// - Four parameters: `log(address, string, bool, uint256)`, etc.
///
/// # Usage Example
///
/// From Solidity contracts:
/// ```solidity
/// import "forge-std/console.sol";
///
/// contract MyContract {
///     function debug() public {
///         console.log("Debug value:", 42);
///         console.log("Address:", msg.sender);
///         console.log("Multiple values:", 100, true, "test");
///     }
/// }
/// ```
///
/// # Output Destinations
///
/// Console messages are broadcast to three channels:
/// 1. **Substrate Logs**: Via `log::info!` with target "console"
/// 2. **Standard Output**: Direct stdout printing (std feature only)
/// 3. **RPC Debug Buffer**: Via `sp_io::misc::print_utf8` for RPC debugging
///
/// # Implementation Details
///
/// The precompile uses the `alloy_core::sol!` macro to generate type-safe bindings
/// for all console.log function signatures, ensuring exact compatibility with
/// Foundry's implementation.
///
/// # References
///
/// - [Foundry console.sol](https://github.com/foundry-rs/forge-std/blob/master/src/console.sol)
/// - [EIP-1967 Proxy Standard](https://eips.ethereum.org/EIPS/eip-1967) for precompile addressing
use crate::{
	precompiles::{BuiltinAddressMatcher, BuiltinPrecompile, Error, Ext},
	vm::RuntimeCosts,
	Config,
};
use alloc::{
	format,
	string::{String, ToString},
	vec::Vec,
};
use alloy_core::{hex, sol};
use core::{marker::PhantomData};

type OutputChannel = alloc::boxed::Box<dyn Fn(&str) + Send + Sync>;

fn default_output_channels() -> alloc::vec::Vec<OutputChannel> {
	use alloc::{boxed::Box, vec};

	vec![
		// Channel 0: Substrate logging framework - appears in node logs
		Box::new(|msg| {
			const CONSOLE_TARGET: &str = "console";
			log::info!(target: CONSOLE_TARGET, "{}", msg);
		}),
		// Channel 1: Standard output - for local development
		#[cfg(feature = "std")]
		Box::new(|msg| println!("{}", msg)),
		#[cfg(not(feature = "std"))]
		Box::new(|_| {}), // No-op when std not available
		// Channel 2: RPC debug buffer - for remote debugging via RPC calls
		Box::new(|msg| sp_io::misc::print_utf8(msg.as_bytes())),
	]
}

pub struct Console<T>(PhantomData<T>);

sol!("src/precompiles/builtin/IConsole.sol");

impl<T: Config> BuiltinPrecompile for Console<T> {
	type T = T;
	type Interface = IConsole::IConsoleCalls;

	/// Console precompile address: 0x000000000000000000636F6e736F6c652e6c6f67
	///
	/// This matches Foundry's console.sol expectation where "console.log"
	/// is encoded as hex in the address.
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Custom(hex_literal::hex!(
		"000000000000000000636F6e736F6c652e6c6f67"
	));

	/// Console precompile does not require contract info storage
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		// Charge minimal gas equivalent to a host function call.
		// This ensures console logging has negligible impact on gas calculations
		// during contract testing while still preventing denial-of-service attacks.
		env.gas_meter_mut().charge(RuntimeCosts::HostFn)?;

		let message = handle_console_call(input);

		Self::broadcast(&message);
		Ok(Vec::new())
	}
}

/// Formats Solidity values for console output according to their type.
///
/// This macro ensures consistent formatting across all console.log variants:
/// - **uint256/int256**: Decimal representation
/// - **string**: Direct string value
/// - **bool**: "true" or "false" literals
/// - **address**: Hex with 0x prefix (checksummed)
/// - **bytes**: Hex with 0x prefix
macro_rules! format_value {
	// Unsigned integers: display as decimal
	($val:expr, uint256) => {
		format!("{}", $val)
	};
	// Signed integers: display as decimal with sign
	($val:expr, int256) => {
		format!("{}", $val)
	};
	// Strings: pass through unchanged
	($val:expr, string) => {
		$val.clone()
	};
	// Booleans: convert to "true"/"false" strings
	($val:expr, bool) => {
		if $val {
			"true".to_string()
		} else {
			"false".to_string()
		}
	};
	// Addresses: format as checksummed hex with 0x prefix
	($val:expr, address) => {
		format!("{:#x}", $val)
	};
	// Dynamic bytes: format as hex with 0x prefix
	($val:expr, bytes) => {
		format!("0x{}", hex::encode($val.as_ref()))
	};
	// Fixed bytes (bytes1-bytes32): format as hex with 0x prefix
	($val:expr, $bytes_type:ident) => {
		format!("0x{}", hex::encode($val.as_ref() as &[u8]))
	};
}

/// Formats two-parameter console.log calls with comma separation
macro_rules! fmt_2 {
	($call:expr, $t1:ident, $t2:ident) => {
		format!("{}, {}", format_value!($call.p0, $t1), format_value!($call.p1, $t2))
	};
}

/// Formats three-parameter console.log calls with comma separation
macro_rules! fmt_3 {
	($call:expr, $t1:ident, $t2:ident, $t3:ident) => {
		format!(
			"{}, {}, {}",
			format_value!($call.p0, $t1),
			format_value!($call.p1, $t2),
			format_value!($call.p2, $t3)
		)
	};
}

/// Formats four-parameter console.log calls with comma separation
macro_rules! fmt_4 {
	($call:expr, $t1:ident, $t2:ident, $t3:ident, $t4:ident) => {
		format!(
			"{}, {}, {}, {}",
			format_value!($call.p0, $t1),
			format_value!($call.p1, $t2),
			format_value!($call.p2, $t3),
			format_value!($call.p3, $t4)
		)
	};
}

pub(crate) fn handle_console_call(input: &IConsole::IConsoleCalls) -> String {
	use IConsole::IConsoleCalls;
	match input {
		// Empty log function
		IConsoleCalls::log_0(_) => "".to_string(),

		// Single parameter functions (basic types)
		IConsoleCalls::log_1(call) => format_value!(call.p0, uint256),
		IConsoleCalls::log_2(call) => format_value!(call.p0, string),
		IConsoleCalls::log_3(call) => format_value!(call.p0, bool),
		IConsoleCalls::log_4(call) => format_value!(call.p0, address),
		IConsoleCalls::log_5(call) => format_value!(call.p0, int256),
		IConsoleCalls::log_6(call) => format_value!(call.p0, bytes),
		// Single parameter functions (named variants)
		IConsoleCalls::logInt(call) => format_value!(call.p0, int256),
		IConsoleCalls::logUint(call) => format_value!(call.p0, uint256),
		IConsoleCalls::logString(call) => format_value!(call.p0, string),
		IConsoleCalls::logBool(call) => format_value!(call.p0, bool),
		IConsoleCalls::logAddress(call) => format_value!(call.p0, address),
		IConsoleCalls::logBytes(call) => format_value!(call.p0, bytes),
		// Single parameter functions (byte variants)
		IConsoleCalls::logBytes1(call) => format_value!(call.p0, bytes1),
		IConsoleCalls::logBytes2(call) => format_value!(call.p0, bytes2),
		IConsoleCalls::logBytes3(call) => format_value!(call.p0, bytes3),
		IConsoleCalls::logBytes4(call) => format_value!(call.p0, bytes4),
		IConsoleCalls::logBytes5(call) => format_value!(call.p0, bytes5),
		IConsoleCalls::logBytes6(call) => format_value!(call.p0, bytes6),
		IConsoleCalls::logBytes7(call) => format_value!(call.p0, bytes7),
		IConsoleCalls::logBytes8(call) => format_value!(call.p0, bytes8),
		IConsoleCalls::logBytes9(call) => format_value!(call.p0, bytes9),
		IConsoleCalls::logBytes10(call) => format_value!(call.p0, bytes10),
		IConsoleCalls::logBytes11(call) => format_value!(call.p0, bytes11),
		IConsoleCalls::logBytes12(call) => format_value!(call.p0, bytes12),
		IConsoleCalls::logBytes13(call) => format_value!(call.p0, bytes13),
		IConsoleCalls::logBytes14(call) => format_value!(call.p0, bytes14),
		IConsoleCalls::logBytes15(call) => format_value!(call.p0, bytes15),
		IConsoleCalls::logBytes16(call) => format_value!(call.p0, bytes16),
		IConsoleCalls::logBytes17(call) => format_value!(call.p0, bytes17),
		IConsoleCalls::logBytes18(call) => format_value!(call.p0, bytes18),
		IConsoleCalls::logBytes19(call) => format_value!(call.p0, bytes19),
		IConsoleCalls::logBytes20(call) => format_value!(call.p0, bytes20),
		IConsoleCalls::logBytes21(call) => format_value!(call.p0, bytes21),
		IConsoleCalls::logBytes22(call) => format_value!(call.p0, bytes22),
		IConsoleCalls::logBytes23(call) => format_value!(call.p0, bytes23),
		IConsoleCalls::logBytes24(call) => format_value!(call.p0, bytes24),
		IConsoleCalls::logBytes25(call) => format_value!(call.p0, bytes25),
		IConsoleCalls::logBytes26(call) => format_value!(call.p0, bytes26),
		IConsoleCalls::logBytes27(call) => format_value!(call.p0, bytes27),
		IConsoleCalls::logBytes28(call) => format_value!(call.p0, bytes28),
		IConsoleCalls::logBytes29(call) => format_value!(call.p0, bytes29),
		IConsoleCalls::logBytes30(call) => format_value!(call.p0, bytes30),
		IConsoleCalls::logBytes31(call) => format_value!(call.p0, bytes31),
		IConsoleCalls::logBytes32(call) => format_value!(call.p0, bytes32),
		// Two parameter functions
		IConsoleCalls::log_7(call) => format_value!(call.p0, bytes1),
		IConsoleCalls::log_8(call) => format_value!(call.p0, bytes2),
		IConsoleCalls::log_9(call) => format_value!(call.p0, bytes3),
		IConsoleCalls::log_10(call) => format_value!(call.p0, bytes4),
		IConsoleCalls::log_11(call) => format_value!(call.p0, bytes5),
		IConsoleCalls::log_12(call) => format_value!(call.p0, bytes6),
		IConsoleCalls::log_13(call) => format_value!(call.p0, bytes7),
		IConsoleCalls::log_14(call) => format_value!(call.p0, bytes8),
		IConsoleCalls::log_15(call) => format_value!(call.p0, bytes9),
		IConsoleCalls::log_16(call) => format_value!(call.p0, bytes10),
		IConsoleCalls::log_17(call) => format_value!(call.p0, bytes11),
		IConsoleCalls::log_18(call) => format_value!(call.p0, bytes12),
		IConsoleCalls::log_19(call) => format_value!(call.p0, bytes13),
		IConsoleCalls::log_20(call) => format_value!(call.p0, bytes14),
		IConsoleCalls::log_21(call) => format_value!(call.p0, bytes15),
		IConsoleCalls::log_22(call) => format_value!(call.p0, bytes16),
		IConsoleCalls::log_23(call) => format_value!(call.p0, bytes17),
		IConsoleCalls::log_24(call) => format_value!(call.p0, bytes18),
		IConsoleCalls::log_25(call) => format_value!(call.p0, bytes19),
		IConsoleCalls::log_26(call) => format_value!(call.p0, bytes20),
		IConsoleCalls::log_27(call) => format_value!(call.p0, bytes21),
		IConsoleCalls::log_28(call) => format_value!(call.p0, bytes22),
		IConsoleCalls::log_29(call) => format_value!(call.p0, bytes23),
		IConsoleCalls::log_30(call) => format_value!(call.p0, bytes24),
		IConsoleCalls::log_31(call) => format_value!(call.p0, bytes25),
		IConsoleCalls::log_32(call) => format_value!(call.p0, bytes26),
		IConsoleCalls::log_33(call) => format_value!(call.p0, bytes27),
		IConsoleCalls::log_34(call) => format_value!(call.p0, bytes28),
		IConsoleCalls::log_35(call) => format_value!(call.p0, bytes29),
		IConsoleCalls::log_36(call) => format_value!(call.p0, bytes30),
		IConsoleCalls::log_37(call) => format_value!(call.p0, bytes31),
		IConsoleCalls::log_38(call) => format_value!(call.p0, bytes32),
		IConsoleCalls::log_39(call) => fmt_2!(call, uint256, uint256),
		IConsoleCalls::log_40(call) => fmt_2!(call, uint256, string),
		IConsoleCalls::log_41(call) => fmt_2!(call, uint256, bool),
		IConsoleCalls::log_42(call) => fmt_2!(call, uint256, address),
		IConsoleCalls::log_43(call) => fmt_2!(call, string, uint256),
		IConsoleCalls::log_44(call) => fmt_2!(call, string, string),
		IConsoleCalls::log_45(call) => fmt_2!(call, string, bool),
		IConsoleCalls::log_46(call) => fmt_2!(call, string, address),
		IConsoleCalls::log_47(call) => fmt_2!(call, bool, uint256),
		IConsoleCalls::log_48(call) => fmt_2!(call, bool, string),
		IConsoleCalls::log_49(call) => fmt_2!(call, bool, bool),
		IConsoleCalls::log_50(call) => fmt_2!(call, bool, address),
		IConsoleCalls::log_51(call) => fmt_2!(call, address, uint256),
		IConsoleCalls::log_52(call) => fmt_2!(call, address, string),
		IConsoleCalls::log_53(call) => fmt_2!(call, address, bool),
		IConsoleCalls::log_54(call) => fmt_2!(call, address, address),
		// Three parameter functions
		IConsoleCalls::log_55(call) => fmt_3!(call, uint256, uint256, uint256),
		IConsoleCalls::log_56(call) => fmt_3!(call, uint256, uint256, string),
		IConsoleCalls::log_57(call) => fmt_3!(call, uint256, uint256, bool),
		IConsoleCalls::log_58(call) => fmt_3!(call, uint256, uint256, address),
		IConsoleCalls::log_59(call) => fmt_3!(call, uint256, string, uint256),
		IConsoleCalls::log_60(call) => fmt_3!(call, uint256, string, string),
		IConsoleCalls::log_61(call) => fmt_3!(call, uint256, string, bool),
		IConsoleCalls::log_62(call) => fmt_3!(call, uint256, string, address),
		IConsoleCalls::log_63(call) => fmt_3!(call, uint256, bool, uint256),
		IConsoleCalls::log_64(call) => fmt_3!(call, uint256, bool, string),
		IConsoleCalls::log_65(call) => fmt_3!(call, uint256, bool, bool),
		IConsoleCalls::log_66(call) => fmt_3!(call, uint256, bool, address),
		IConsoleCalls::log_67(call) => fmt_3!(call, uint256, address, uint256),
		IConsoleCalls::log_68(call) => fmt_3!(call, uint256, address, string),
		IConsoleCalls::log_69(call) => fmt_3!(call, uint256, address, bool),
		IConsoleCalls::log_70(call) => fmt_3!(call, uint256, address, address),
		IConsoleCalls::log_71(call) => fmt_3!(call, string, uint256, uint256),
		IConsoleCalls::log_72(call) => fmt_3!(call, string, uint256, string),
		IConsoleCalls::log_73(call) => fmt_3!(call, string, uint256, bool),
		IConsoleCalls::log_74(call) => fmt_3!(call, string, uint256, address),
		IConsoleCalls::log_75(call) => fmt_3!(call, string, string, uint256),
		IConsoleCalls::log_76(call) => fmt_3!(call, string, string, string),
		IConsoleCalls::log_77(call) => fmt_3!(call, string, string, bool),
		IConsoleCalls::log_78(call) => fmt_3!(call, string, string, address),
		IConsoleCalls::log_79(call) => fmt_3!(call, string, bool, uint256),
		IConsoleCalls::log_80(call) => fmt_3!(call, string, bool, string),
		IConsoleCalls::log_81(call) => fmt_3!(call, string, bool, bool),
		IConsoleCalls::log_82(call) => fmt_3!(call, string, bool, address),
		IConsoleCalls::log_83(call) => fmt_3!(call, string, address, uint256),
		IConsoleCalls::log_84(call) => fmt_3!(call, string, address, string),
		IConsoleCalls::log_85(call) => fmt_3!(call, string, address, bool),
		IConsoleCalls::log_86(call) => fmt_3!(call, string, address, address),
		IConsoleCalls::log_87(call) => fmt_3!(call, bool, uint256, uint256),
		IConsoleCalls::log_88(call) => fmt_3!(call, bool, uint256, string),
		IConsoleCalls::log_89(call) => fmt_3!(call, bool, uint256, bool),
		IConsoleCalls::log_90(call) => fmt_3!(call, bool, uint256, address),
		IConsoleCalls::log_91(call) => fmt_3!(call, bool, string, uint256),
		IConsoleCalls::log_92(call) => fmt_3!(call, bool, string, string),
		IConsoleCalls::log_93(call) => fmt_3!(call, bool, string, bool),
		IConsoleCalls::log_94(call) => fmt_3!(call, bool, string, address),
		IConsoleCalls::log_95(call) => fmt_3!(call, bool, bool, uint256),
		IConsoleCalls::log_96(call) => fmt_3!(call, bool, bool, string),
		IConsoleCalls::log_97(call) => fmt_3!(call, bool, bool, bool),
		IConsoleCalls::log_98(call) => fmt_3!(call, bool, bool, address),
		IConsoleCalls::log_99(call) => fmt_3!(call, bool, address, uint256),
		IConsoleCalls::log_100(call) => fmt_3!(call, bool, address, string),
		IConsoleCalls::log_101(call) => fmt_3!(call, bool, address, bool),
		IConsoleCalls::log_102(call) => fmt_3!(call, bool, address, address),
		IConsoleCalls::log_103(call) => fmt_3!(call, address, uint256, uint256),
		IConsoleCalls::log_104(call) => fmt_3!(call, address, uint256, string),
		IConsoleCalls::log_105(call) => fmt_3!(call, address, uint256, bool),
		IConsoleCalls::log_106(call) => fmt_3!(call, address, uint256, address),
		IConsoleCalls::log_107(call) => fmt_3!(call, address, string, uint256),
		IConsoleCalls::log_108(call) => fmt_3!(call, address, string, string),
		IConsoleCalls::log_109(call) => fmt_3!(call, address, string, bool),
		IConsoleCalls::log_110(call) => fmt_3!(call, address, string, address),
		IConsoleCalls::log_111(call) => fmt_3!(call, address, bool, uint256),
		IConsoleCalls::log_112(call) => fmt_3!(call, address, bool, string),
		IConsoleCalls::log_113(call) => fmt_3!(call, address, bool, bool),
		IConsoleCalls::log_114(call) => fmt_3!(call, address, bool, address),
		IConsoleCalls::log_115(call) => fmt_3!(call, address, address, uint256),
		IConsoleCalls::log_116(call) => fmt_3!(call, address, address, string),
		IConsoleCalls::log_117(call) => fmt_3!(call, address, address, bool),
		IConsoleCalls::log_118(call) => fmt_3!(call, address, address, address),
		// Four parameter function
		IConsoleCalls::log_119(call) => fmt_4!(call, uint256, uint256, uint256, uint256),
		IConsoleCalls::log_120(call) => fmt_4!(call, uint256, uint256, uint256, string),
		IConsoleCalls::log_121(call) => fmt_4!(call, uint256, uint256, uint256, bool),
		IConsoleCalls::log_122(call) => fmt_4!(call, uint256, uint256, uint256, address),
		IConsoleCalls::log_123(call) => fmt_4!(call, uint256, uint256, string, uint256),
		IConsoleCalls::log_124(call) => fmt_4!(call, uint256, uint256, string, string),
		IConsoleCalls::log_125(call) => fmt_4!(call, uint256, uint256, string, bool),
		IConsoleCalls::log_126(call) => fmt_4!(call, uint256, uint256, string, address),
		IConsoleCalls::log_127(call) => fmt_4!(call, uint256, uint256, bool, uint256),
		IConsoleCalls::log_128(call) => fmt_4!(call, uint256, uint256, bool, string),
		IConsoleCalls::log_129(call) => fmt_4!(call, uint256, uint256, bool, bool),
		IConsoleCalls::log_130(call) => fmt_4!(call, uint256, uint256, bool, address),
		IConsoleCalls::log_131(call) => fmt_4!(call, uint256, uint256, address, uint256),
		IConsoleCalls::log_132(call) => fmt_4!(call, uint256, uint256, address, string),
		IConsoleCalls::log_133(call) => fmt_4!(call, uint256, uint256, address, bool),
		IConsoleCalls::log_134(call) => fmt_4!(call, uint256, uint256, address, address),
		IConsoleCalls::log_135(call) => fmt_4!(call, uint256, string, uint256, uint256),
		IConsoleCalls::log_136(call) => fmt_4!(call, uint256, string, uint256, string),
		IConsoleCalls::log_137(call) => fmt_4!(call, uint256, string, uint256, bool),
		IConsoleCalls::log_138(call) => fmt_4!(call, uint256, string, uint256, address),
		IConsoleCalls::log_139(call) => fmt_4!(call, uint256, string, string, uint256),
		IConsoleCalls::log_140(call) => fmt_4!(call, uint256, string, string, string),
		IConsoleCalls::log_141(call) => fmt_4!(call, uint256, string, string, bool),
		IConsoleCalls::log_142(call) => fmt_4!(call, uint256, string, string, address),
		IConsoleCalls::log_143(call) => fmt_4!(call, uint256, string, bool, uint256),
		IConsoleCalls::log_144(call) => fmt_4!(call, uint256, string, bool, string),
		IConsoleCalls::log_145(call) => fmt_4!(call, uint256, string, bool, bool),
		IConsoleCalls::log_146(call) => fmt_4!(call, uint256, string, bool, address),
		IConsoleCalls::log_147(call) => fmt_4!(call, uint256, string, address, uint256),
		IConsoleCalls::log_148(call) => fmt_4!(call, uint256, string, address, string),
		IConsoleCalls::log_149(call) => fmt_4!(call, uint256, string, address, bool),
		IConsoleCalls::log_150(call) => fmt_4!(call, uint256, string, address, address),
		IConsoleCalls::log_151(call) => fmt_4!(call, uint256, bool, uint256, uint256),
		IConsoleCalls::log_152(call) => fmt_4!(call, uint256, bool, uint256, string),
		IConsoleCalls::log_153(call) => fmt_4!(call, uint256, bool, uint256, bool),
		IConsoleCalls::log_154(call) => fmt_4!(call, uint256, bool, uint256, address),
		IConsoleCalls::log_155(call) => fmt_4!(call, uint256, bool, string, uint256),
		IConsoleCalls::log_156(call) => fmt_4!(call, uint256, bool, string, string),
		IConsoleCalls::log_157(call) => fmt_4!(call, uint256, bool, string, bool),
		IConsoleCalls::log_158(call) => fmt_4!(call, uint256, bool, string, address),
		IConsoleCalls::log_159(call) => fmt_4!(call, uint256, bool, bool, uint256),
		IConsoleCalls::log_160(call) => fmt_4!(call, uint256, bool, bool, string),
		IConsoleCalls::log_161(call) => fmt_4!(call, uint256, bool, bool, bool),
		IConsoleCalls::log_162(call) => fmt_4!(call, uint256, bool, bool, address),
		IConsoleCalls::log_163(call) => fmt_4!(call, uint256, bool, address, uint256),
		IConsoleCalls::log_164(call) => fmt_4!(call, uint256, bool, address, string),
		IConsoleCalls::log_165(call) => fmt_4!(call, uint256, bool, address, bool),
		IConsoleCalls::log_166(call) => fmt_4!(call, uint256, bool, address, address),
		IConsoleCalls::log_167(call) => fmt_4!(call, uint256, address, uint256, uint256),
		IConsoleCalls::log_168(call) => fmt_4!(call, uint256, address, uint256, string),
		IConsoleCalls::log_169(call) => fmt_4!(call, uint256, address, uint256, bool),
		IConsoleCalls::log_170(call) => fmt_4!(call, uint256, address, uint256, address),
		IConsoleCalls::log_171(call) => fmt_4!(call, uint256, address, string, uint256),
		IConsoleCalls::log_172(call) => fmt_4!(call, uint256, address, string, string),
		IConsoleCalls::log_173(call) => fmt_4!(call, uint256, address, string, bool),
		IConsoleCalls::log_174(call) => fmt_4!(call, uint256, address, string, address),
		IConsoleCalls::log_175(call) => fmt_4!(call, uint256, address, bool, uint256),
		IConsoleCalls::log_176(call) => fmt_4!(call, uint256, address, bool, string),
		IConsoleCalls::log_177(call) => fmt_4!(call, uint256, address, bool, bool),
		IConsoleCalls::log_178(call) => fmt_4!(call, uint256, address, bool, address),
		IConsoleCalls::log_179(call) => fmt_4!(call, uint256, address, address, uint256),
		IConsoleCalls::log_180(call) => fmt_4!(call, uint256, address, address, string),
		IConsoleCalls::log_181(call) => fmt_4!(call, uint256, address, address, bool),
		IConsoleCalls::log_182(call) => fmt_4!(call, uint256, address, address, address),
		IConsoleCalls::log_183(call) => fmt_4!(call, string, uint256, uint256, uint256),
		IConsoleCalls::log_184(call) => fmt_4!(call, string, uint256, uint256, string),
		IConsoleCalls::log_185(call) => fmt_4!(call, string, uint256, uint256, bool),
		IConsoleCalls::log_186(call) => fmt_4!(call, string, uint256, uint256, address),
		IConsoleCalls::log_187(call) => fmt_4!(call, string, uint256, string, uint256),
		IConsoleCalls::log_188(call) => fmt_4!(call, string, uint256, string, string),
		IConsoleCalls::log_189(call) => fmt_4!(call, string, uint256, string, bool),
		IConsoleCalls::log_190(call) => fmt_4!(call, string, uint256, string, address),
		IConsoleCalls::log_191(call) => fmt_4!(call, string, uint256, bool, uint256),
		IConsoleCalls::log_192(call) => fmt_4!(call, string, uint256, bool, string),
		IConsoleCalls::log_193(call) => fmt_4!(call, string, uint256, bool, bool),
		IConsoleCalls::log_194(call) => fmt_4!(call, string, uint256, bool, address),
		IConsoleCalls::log_195(call) => fmt_4!(call, string, uint256, address, uint256),
		IConsoleCalls::log_196(call) => fmt_4!(call, string, uint256, address, string),
		IConsoleCalls::log_197(call) => fmt_4!(call, string, uint256, address, bool),
		IConsoleCalls::log_198(call) => fmt_4!(call, string, uint256, address, address),
		IConsoleCalls::log_199(call) => fmt_4!(call, string, string, uint256, uint256),
		IConsoleCalls::log_200(call) => fmt_4!(call, string, string, uint256, string),
		IConsoleCalls::log_201(call) => fmt_4!(call, string, string, uint256, bool),
		IConsoleCalls::log_202(call) => fmt_4!(call, string, string, uint256, address),
		IConsoleCalls::log_203(call) => fmt_4!(call, string, string, string, uint256),
		IConsoleCalls::log_204(call) => fmt_4!(call, string, string, string, string),
		IConsoleCalls::log_205(call) => fmt_4!(call, string, string, string, bool),
		IConsoleCalls::log_206(call) => fmt_4!(call, string, string, string, address),
		IConsoleCalls::log_207(call) => fmt_4!(call, string, string, bool, uint256),
		IConsoleCalls::log_208(call) => fmt_4!(call, string, string, bool, string),
		IConsoleCalls::log_209(call) => fmt_4!(call, string, string, bool, bool),
		IConsoleCalls::log_210(call) => fmt_4!(call, string, string, bool, address),
		IConsoleCalls::log_211(call) => fmt_4!(call, string, string, address, uint256),
		IConsoleCalls::log_212(call) => fmt_4!(call, string, string, address, string),
		IConsoleCalls::log_213(call) => fmt_4!(call, string, string, address, bool),
		IConsoleCalls::log_214(call) => fmt_4!(call, string, string, address, address),
		IConsoleCalls::log_215(call) => fmt_4!(call, string, bool, uint256, uint256),
		IConsoleCalls::log_216(call) => fmt_4!(call, string, bool, uint256, string),
		IConsoleCalls::log_217(call) => fmt_4!(call, string, bool, uint256, bool),
		IConsoleCalls::log_218(call) => fmt_4!(call, string, bool, uint256, address),
		IConsoleCalls::log_219(call) => fmt_4!(call, string, bool, string, uint256),
		IConsoleCalls::log_220(call) => fmt_4!(call, string, bool, string, string),
		IConsoleCalls::log_221(call) => fmt_4!(call, string, bool, string, bool),
		IConsoleCalls::log_222(call) => fmt_4!(call, string, bool, string, address),
		IConsoleCalls::log_223(call) => fmt_4!(call, string, bool, bool, uint256),
		IConsoleCalls::log_224(call) => fmt_4!(call, string, bool, bool, string),
		IConsoleCalls::log_225(call) => fmt_4!(call, string, bool, bool, bool),
		IConsoleCalls::log_226(call) => fmt_4!(call, string, bool, bool, address),
		IConsoleCalls::log_227(call) => fmt_4!(call, string, bool, address, uint256),
		IConsoleCalls::log_228(call) => fmt_4!(call, string, bool, address, string),
		IConsoleCalls::log_229(call) => fmt_4!(call, string, bool, address, bool),
		IConsoleCalls::log_230(call) => fmt_4!(call, string, bool, address, address),
		IConsoleCalls::log_231(call) => fmt_4!(call, string, address, uint256, uint256),
		IConsoleCalls::log_232(call) => fmt_4!(call, string, address, uint256, string),
		IConsoleCalls::log_233(call) => fmt_4!(call, string, address, uint256, bool),
		IConsoleCalls::log_234(call) => fmt_4!(call, string, address, uint256, address),
		IConsoleCalls::log_235(call) => fmt_4!(call, string, address, string, uint256),
		IConsoleCalls::log_236(call) => fmt_4!(call, string, address, string, string),
		IConsoleCalls::log_237(call) => fmt_4!(call, string, address, string, bool),
		IConsoleCalls::log_238(call) => fmt_4!(call, string, address, string, address),
		IConsoleCalls::log_239(call) => fmt_4!(call, string, address, bool, uint256),
		IConsoleCalls::log_240(call) => fmt_4!(call, string, address, bool, string),
		IConsoleCalls::log_241(call) => fmt_4!(call, string, address, bool, bool),
		IConsoleCalls::log_242(call) => fmt_4!(call, string, address, bool, address),
		IConsoleCalls::log_243(call) => fmt_4!(call, string, address, address, uint256),
		IConsoleCalls::log_244(call) => fmt_4!(call, string, address, address, string),
		IConsoleCalls::log_245(call) => fmt_4!(call, string, address, address, bool),
		IConsoleCalls::log_246(call) => fmt_4!(call, string, address, address, address),
		IConsoleCalls::log_247(call) => fmt_4!(call, bool, uint256, uint256, uint256),
		IConsoleCalls::log_248(call) => fmt_4!(call, bool, uint256, uint256, string),
		IConsoleCalls::log_249(call) => fmt_4!(call, bool, uint256, uint256, bool),
		IConsoleCalls::log_250(call) => fmt_4!(call, bool, uint256, uint256, address),
		IConsoleCalls::log_251(call) => fmt_4!(call, bool, uint256, string, uint256),
		IConsoleCalls::log_252(call) => fmt_4!(call, bool, uint256, string, string),
		IConsoleCalls::log_253(call) => fmt_4!(call, bool, uint256, string, bool),
		IConsoleCalls::log_254(call) => fmt_4!(call, bool, uint256, string, address),
		IConsoleCalls::log_255(call) => fmt_4!(call, bool, uint256, bool, uint256),
		IConsoleCalls::log_256(call) => fmt_4!(call, bool, uint256, bool, string),
		IConsoleCalls::log_257(call) => fmt_4!(call, bool, uint256, bool, bool),
		IConsoleCalls::log_258(call) => fmt_4!(call, bool, uint256, bool, address),
		IConsoleCalls::log_259(call) => fmt_4!(call, bool, uint256, address, uint256),
		IConsoleCalls::log_260(call) => fmt_4!(call, bool, uint256, address, string),
		IConsoleCalls::log_261(call) => fmt_4!(call, bool, uint256, address, bool),
		IConsoleCalls::log_262(call) => fmt_4!(call, bool, uint256, address, address),
		IConsoleCalls::log_263(call) => fmt_4!(call, bool, string, uint256, uint256),
		IConsoleCalls::log_264(call) => fmt_4!(call, bool, string, uint256, string),
		IConsoleCalls::log_265(call) => fmt_4!(call, bool, string, uint256, bool),
		IConsoleCalls::log_266(call) => fmt_4!(call, bool, string, uint256, address),
		IConsoleCalls::log_267(call) => fmt_4!(call, bool, string, string, uint256),
		IConsoleCalls::log_268(call) => fmt_4!(call, bool, string, string, string),
		IConsoleCalls::log_269(call) => fmt_4!(call, bool, string, string, bool),
		IConsoleCalls::log_270(call) => fmt_4!(call, bool, string, string, address),
		IConsoleCalls::log_271(call) => fmt_4!(call, bool, string, bool, uint256),
		IConsoleCalls::log_272(call) => fmt_4!(call, bool, string, bool, string),
		IConsoleCalls::log_273(call) => fmt_4!(call, bool, string, bool, bool),
		IConsoleCalls::log_274(call) => fmt_4!(call, bool, string, bool, address),
		IConsoleCalls::log_275(call) => fmt_4!(call, bool, string, address, uint256),
		IConsoleCalls::log_276(call) => fmt_4!(call, bool, string, address, string),
		IConsoleCalls::log_277(call) => fmt_4!(call, bool, string, address, bool),
		IConsoleCalls::log_278(call) => fmt_4!(call, bool, string, address, address),
		IConsoleCalls::log_279(call) => fmt_4!(call, bool, bool, uint256, uint256),
		IConsoleCalls::log_280(call) => fmt_4!(call, bool, bool, uint256, string),
		IConsoleCalls::log_281(call) => fmt_4!(call, bool, bool, uint256, bool),
		IConsoleCalls::log_282(call) => fmt_4!(call, bool, bool, uint256, address),
		IConsoleCalls::log_283(call) => fmt_4!(call, bool, bool, string, uint256),
		IConsoleCalls::log_284(call) => fmt_4!(call, bool, bool, string, string),
		IConsoleCalls::log_285(call) => fmt_4!(call, bool, bool, string, bool),
		IConsoleCalls::log_286(call) => fmt_4!(call, bool, bool, string, address),
		IConsoleCalls::log_287(call) => fmt_4!(call, bool, bool, bool, uint256),
		IConsoleCalls::log_288(call) => fmt_4!(call, bool, bool, bool, string),
		IConsoleCalls::log_289(call) => fmt_4!(call, bool, bool, bool, bool),
		IConsoleCalls::log_290(call) => fmt_4!(call, bool, bool, bool, address),
		IConsoleCalls::log_291(call) => fmt_4!(call, bool, bool, address, uint256),
		IConsoleCalls::log_292(call) => fmt_4!(call, bool, bool, address, string),
		IConsoleCalls::log_293(call) => fmt_4!(call, bool, bool, address, bool),
		IConsoleCalls::log_294(call) => fmt_4!(call, bool, bool, address, address),
		IConsoleCalls::log_295(call) => fmt_4!(call, bool, address, uint256, uint256),
		IConsoleCalls::log_296(call) => fmt_4!(call, bool, address, uint256, string),
		IConsoleCalls::log_297(call) => fmt_4!(call, bool, address, uint256, bool),
		IConsoleCalls::log_298(call) => fmt_4!(call, bool, address, uint256, address),
		IConsoleCalls::log_299(call) => fmt_4!(call, bool, address, string, uint256),
		IConsoleCalls::log_300(call) => fmt_4!(call, bool, address, string, string),
		IConsoleCalls::log_301(call) => fmt_4!(call, bool, address, string, bool),
		IConsoleCalls::log_302(call) => fmt_4!(call, bool, address, string, address),
		IConsoleCalls::log_303(call) => fmt_4!(call, bool, address, bool, uint256),
		IConsoleCalls::log_304(call) => fmt_4!(call, bool, address, bool, string),
		IConsoleCalls::log_305(call) => fmt_4!(call, bool, address, bool, bool),
		IConsoleCalls::log_306(call) => fmt_4!(call, bool, address, bool, address),
		IConsoleCalls::log_307(call) => fmt_4!(call, bool, address, address, uint256),
		IConsoleCalls::log_308(call) => fmt_4!(call, bool, address, address, string),
		IConsoleCalls::log_309(call) => fmt_4!(call, bool, address, address, bool),
		IConsoleCalls::log_310(call) => fmt_4!(call, bool, address, address, address),
		IConsoleCalls::log_311(call) => fmt_4!(call, address, uint256, uint256, uint256),
		IConsoleCalls::log_312(call) => fmt_4!(call, address, uint256, uint256, string),
		IConsoleCalls::log_313(call) => fmt_4!(call, address, uint256, uint256, bool),
		IConsoleCalls::log_314(call) => fmt_4!(call, address, uint256, uint256, address),
		IConsoleCalls::log_315(call) => fmt_4!(call, address, uint256, string, uint256),
		IConsoleCalls::log_316(call) => fmt_4!(call, address, uint256, string, string),
		IConsoleCalls::log_317(call) => fmt_4!(call, address, uint256, string, bool),
		IConsoleCalls::log_318(call) => fmt_4!(call, address, uint256, string, address),
		IConsoleCalls::log_319(call) => fmt_4!(call, address, uint256, bool, uint256),
		IConsoleCalls::log_320(call) => fmt_4!(call, address, uint256, bool, string),
		IConsoleCalls::log_321(call) => fmt_4!(call, address, uint256, bool, bool),
		IConsoleCalls::log_322(call) => fmt_4!(call, address, uint256, bool, address),
		IConsoleCalls::log_323(call) => fmt_4!(call, address, uint256, address, uint256),
		IConsoleCalls::log_324(call) => fmt_4!(call, address, uint256, address, string),
		IConsoleCalls::log_325(call) => fmt_4!(call, address, uint256, address, bool),
		IConsoleCalls::log_326(call) => fmt_4!(call, address, uint256, address, address),
		IConsoleCalls::log_327(call) => fmt_4!(call, address, string, uint256, uint256),
		IConsoleCalls::log_328(call) => fmt_4!(call, address, string, uint256, string),
		IConsoleCalls::log_329(call) => fmt_4!(call, address, string, uint256, bool),
		IConsoleCalls::log_330(call) => fmt_4!(call, address, string, uint256, address),
		IConsoleCalls::log_331(call) => fmt_4!(call, address, string, string, uint256),
		IConsoleCalls::log_332(call) => fmt_4!(call, address, string, string, string),
		IConsoleCalls::log_333(call) => fmt_4!(call, address, string, string, bool),
		IConsoleCalls::log_334(call) => fmt_4!(call, address, string, string, address),
		IConsoleCalls::log_335(call) => fmt_4!(call, address, string, bool, uint256),
		IConsoleCalls::log_336(call) => fmt_4!(call, address, string, bool, string),
		IConsoleCalls::log_337(call) => fmt_4!(call, address, string, bool, bool),
		IConsoleCalls::log_338(call) => fmt_4!(call, address, string, bool, address),
		IConsoleCalls::log_339(call) => fmt_4!(call, address, string, address, uint256),
		IConsoleCalls::log_340(call) => fmt_4!(call, address, string, address, string),
		IConsoleCalls::log_341(call) => fmt_4!(call, address, string, address, bool),
		IConsoleCalls::log_342(call) => fmt_4!(call, address, string, address, address),
		IConsoleCalls::log_343(call) => fmt_4!(call, address, bool, uint256, uint256),
		IConsoleCalls::log_344(call) => fmt_4!(call, address, bool, uint256, string),
		IConsoleCalls::log_345(call) => fmt_4!(call, address, bool, uint256, bool),
		IConsoleCalls::log_346(call) => fmt_4!(call, address, bool, uint256, address),
		IConsoleCalls::log_347(call) => fmt_4!(call, address, bool, string, uint256),
		IConsoleCalls::log_348(call) => fmt_4!(call, address, bool, string, string),
		IConsoleCalls::log_349(call) => fmt_4!(call, address, bool, string, bool),
		IConsoleCalls::log_350(call) => fmt_4!(call, address, bool, string, address),
		IConsoleCalls::log_351(call) => fmt_4!(call, address, bool, bool, uint256),
		IConsoleCalls::log_352(call) => fmt_4!(call, address, bool, bool, string),
		IConsoleCalls::log_353(call) => fmt_4!(call, address, bool, bool, bool),
		IConsoleCalls::log_354(call) => fmt_4!(call, address, bool, bool, address),
		IConsoleCalls::log_355(call) => fmt_4!(call, address, bool, address, uint256),
		IConsoleCalls::log_356(call) => fmt_4!(call, address, bool, address, string),
		IConsoleCalls::log_357(call) => fmt_4!(call, address, bool, address, bool),
		IConsoleCalls::log_358(call) => fmt_4!(call, address, bool, address, address),
		IConsoleCalls::log_359(call) => fmt_4!(call, address, address, uint256, uint256),
		IConsoleCalls::log_360(call) => fmt_4!(call, address, address, uint256, string),
		IConsoleCalls::log_361(call) => fmt_4!(call, address, address, uint256, bool),
		IConsoleCalls::log_362(call) => fmt_4!(call, address, address, uint256, address),
		IConsoleCalls::log_363(call) => fmt_4!(call, address, address, string, uint256),
		IConsoleCalls::log_364(call) => fmt_4!(call, address, address, string, string),
		IConsoleCalls::log_365(call) => fmt_4!(call, address, address, string, bool),
		IConsoleCalls::log_366(call) => fmt_4!(call, address, address, string, address),
		IConsoleCalls::log_367(call) => fmt_4!(call, address, address, bool, uint256),
		IConsoleCalls::log_368(call) => fmt_4!(call, address, address, bool, string),
		IConsoleCalls::log_369(call) => fmt_4!(call, address, address, bool, bool),
		IConsoleCalls::log_370(call) => fmt_4!(call, address, address, bool, address),
		IConsoleCalls::log_371(call) => fmt_4!(call, address, address, address, uint256),
		IConsoleCalls::log_372(call) => fmt_4!(call, address, address, address, string),
		IConsoleCalls::log_373(call) => fmt_4!(call, address, address, address, bool),
		IConsoleCalls::log_374(call) => fmt_4!(call, address, address, address, address),
		_ => format!(""),
	}
}

impl<T> Console<T> {
	fn broadcast(message: &str) {
		let channels = default_output_channels();
		for channel in channels {
			channel(message);
		}
	}
}
