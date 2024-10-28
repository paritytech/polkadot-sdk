// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Outbound V2 primitives

use crate::outbound::OperatingMode;
use alloy_sol_types::sol;
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::ConstU32, BoundedVec, PalletError};
use hex_literal::hex;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::{RuntimeDebug, H160, H256};
use sp_std::{vec, vec::Vec};

use alloy_primitives::{Address, FixedBytes};
use alloy_sol_types::SolValue;

sol! {
	#[derive(Encode, Decode, RuntimeDebug, TypeInfo)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	struct InboundMessage {
		// origin
		bytes origin;
		// Message nonce
		uint64 nonce;
		// Commands
		CommandWrapper[] commands;
	}

	#[derive(Encode, Decode, RuntimeDebug, TypeInfo)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	struct CommandWrapper {
		uint8 kind;
		uint64 gas;
		bytes payload;
	}

	// Payload for Upgrade
	struct UpgradeParams {
		// The address of the implementation contract
		address implAddress;
		// Codehash of the new implementation contract.
		bytes32 implCodeHash;
		// Parameters used to upgrade storage of the gateway
		bytes initParams;
	}

	// Payload for CreateAgent
	struct CreateAgentParams {
		/// @dev The agent ID of the consensus system
		bytes32 agentID;
	}

	// Payload for SetOperatingMode instruction
	struct SetOperatingModeParams {
		/// The new operating mode
		uint8 mode;
	}

	// Payload for NativeTokenUnlock instruction
	struct UnlockNativeTokenParams {
		// Token address
		address token;
		// Recipient address
		address recipient;
		// Amount to unlock
		uint128 amount;
	}

	// Payload for RegisterForeignToken
	struct RegisterForeignTokenParams {
		/// @dev The token ID (hash of stable location id of token)
		bytes32 foreignTokenID;
		/// @dev The name of the token
		bytes name;
		/// @dev The symbol of the token
		bytes symbol;
		/// @dev The decimal of the token
		uint8 decimals;
	}

	// Payload for MintForeignTokenParams instruction
	struct MintForeignTokenParams {
		// Foreign token ID
		bytes32 foreignTokenID;
		// Recipient address
		address recipient;
		// Amount to mint
		uint128 amount;
	}
}

pub const MAX_COMMANDS: u32 = 8;

/// A message which can be accepted by implementations of `/[`SendMessage`\]`
#[derive(Encode, Decode, TypeInfo, PartialEq, Clone, RuntimeDebug)]
pub struct Message {
	/// Origin
	pub origin: H256,
	/// ID
	pub id: H256,
	/// Fee
	pub fee: u128,
	/// Commands
	pub commands: BoundedVec<Command, ConstU32<MAX_COMMANDS>>,
}

/// A command which is executable by the Gateway contract on Ethereum
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub enum Command {
	/// Upgrade the Gateway contract
	Upgrade {
		/// Address of the new implementation contract
		impl_address: H160,
		/// Codehash of the implementation contract
		impl_code_hash: H256,
		/// Optionally invoke an initializer in the implementation contract
		initializer: Option<Initializer>,
	},
	/// Create an agent representing a consensus system on Polkadot
	CreateAgent {
		/// The ID of the agent, derived from the `MultiLocation` of the consensus system on
		/// Polkadot
		agent_id: H256,
	},
	/// Set the global operating mode of the Gateway contract
	SetOperatingMode {
		/// The new operating mode
		mode: OperatingMode,
	},
	/// Unlock ERC20 tokens
	UnlockNativeToken {
		/// ID of the agent
		agent_id: H256,
		/// Address of the ERC20 token
		token: H160,
		/// The recipient of the tokens
		recipient: H160,
		/// The amount of tokens to transfer
		amount: u128,
	},
	/// Register foreign token from Polkadot
	RegisterForeignToken {
		/// ID for the token
		token_id: H256,
		/// Name of the token
		name: Vec<u8>,
		/// Short symbol for the token
		symbol: Vec<u8>,
		/// Number of decimal places
		decimals: u8,
	},
	/// Mint foreign token from Polkadot
	MintForeignToken {
		/// ID for the token
		token_id: H256,
		/// The recipient of the newly minted tokens
		recipient: H160,
		/// The amount of tokens to mint
		amount: u128,
	},
}

impl Command {
	/// Compute the enum variant index
	pub fn index(&self) -> u8 {
		match self {
			Command::Upgrade { .. } => 0,
			Command::SetOperatingMode { .. } => 1,
			Command::UnlockNativeToken { .. } => 2,
			Command::RegisterForeignToken { .. } => 3,
			Command::MintForeignToken { .. } => 4,
			Command::CreateAgent { .. } => 5,
		}
	}

	/// ABI-encode the Command.
	pub fn abi_encode(&self) -> Vec<u8> {
		match self {
			Command::Upgrade { impl_address, impl_code_hash, initializer, .. } => UpgradeParams {
				implAddress: Address::from(impl_address.as_fixed_bytes()),
				implCodeHash: FixedBytes::from(impl_code_hash.as_fixed_bytes()),
				initParams: initializer.clone().map_or(vec![], |i| i.params),
			}
			.abi_encode(),
			Command::CreateAgent { agent_id } =>
				CreateAgentParams { agentID: FixedBytes::from(agent_id.as_fixed_bytes()) }
					.abi_encode(),
			Command::SetOperatingMode { mode } =>
				SetOperatingModeParams { mode: (*mode) as u8 }.abi_encode(),
			Command::UnlockNativeToken { token, recipient, amount, .. } =>
				UnlockNativeTokenParams {
					token: Address::from(token.as_fixed_bytes()),
					recipient: Address::from(recipient.as_fixed_bytes()),
					amount: *amount,
				}
				.abi_encode(),
			Command::RegisterForeignToken { token_id, name, symbol, decimals } =>
				RegisterForeignTokenParams {
					foreignTokenID: FixedBytes::from(token_id.as_fixed_bytes()),
					name: name.to_vec(),
					symbol: symbol.to_vec(),
					decimals: *decimals,
				}
				.abi_encode(),
			Command::MintForeignToken { token_id, recipient, amount } => MintForeignTokenParams {
				foreignTokenID: FixedBytes::from(token_id.as_fixed_bytes()),
				recipient: Address::from(recipient.as_fixed_bytes()),
				amount: *amount,
			}
			.abi_encode(),
		}
	}
}

/// Representation of a call to the initializer of an implementation contract.
/// The initializer has the following ABI signature: `initialize(bytes)`.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct Initializer {
	/// ABI-encoded params of type `bytes` to pass to the initializer
	pub params: Vec<u8>,
	/// The initializer is allowed to consume this much gas at most.
	pub maximum_required_gas: u64,
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(PartialEq))]
/// Fee for delivering message
pub struct Fee<Balance>
where
	Balance: BaseArithmetic + Unsigned + Copy,
{
	/// Fee to cover cost of processing the message locally
	pub local: Balance,
}

impl<Balance> Fee<Balance>
where
	Balance: BaseArithmetic + Unsigned + Copy,
{
	pub fn total(&self) -> Balance {
		self.local
	}
}

impl<Balance> From<Balance> for Fee<Balance>
where
	Balance: BaseArithmetic + Unsigned + Copy,
{
	fn from(local: Balance) -> Self {
		Self { local }
	}
}

pub trait SendMessage: SendMessageFeeProvider {
	type Ticket: Clone + Encode + Decode;

	/// Validate an outbound message and return a tuple:
	/// 1. Ticket for submitting the message
	/// 2. Delivery fee
	fn validate(
		message: &Message,
	) -> Result<(Self::Ticket, Fee<<Self as SendMessageFeeProvider>::Balance>), SendError>;

	/// Submit the message ticket for eventual delivery to Ethereum
	fn deliver(ticket: Self::Ticket) -> Result<H256, SendError>;
}

/// A trait for getting the local costs associated with sending a message.
pub trait SendMessageFeeProvider {
	type Balance: BaseArithmetic + Unsigned + Copy;

	/// The local component of the message processing fees in native currency
	fn local_fee() -> Self::Balance;
}

/// Reasons why sending to Ethereum could not be initiated
#[derive(Copy, Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, PalletError, TypeInfo)]
pub enum SendError {
	/// Message is too large to be safely executed on Ethereum
	MessageTooLarge,
	/// The bridge has been halted for maintenance
	Halted,
	/// Invalid Channel
	InvalidChannel,
}

pub trait GasMeter {
	/// Measures the maximum amount of gas a command payload will require to *dispatch*, NOT
	/// including validation & verification.
	fn maximum_dispatch_gas_used_at_most(command: &Command) -> u64;
}

/// A meter that assigns a constant amount of gas for the execution of a command
///
/// The gas figures are extracted from this report:
/// > forge test --match-path test/Gateway.t.sol --gas-report
///
/// A healthy buffer is added on top of these figures to account for:
/// * The EIP-150 63/64 rule
/// * Future EVM upgrades that may increase gas cost
pub struct ConstantGasMeter;

impl GasMeter for ConstantGasMeter {
	fn maximum_dispatch_gas_used_at_most(command: &Command) -> u64 {
		match command {
			Command::CreateAgent { .. } => 275_000,
			Command::SetOperatingMode { .. } => 40_000,
			Command::Upgrade { initializer, .. } => {
				let initializer_max_gas = match *initializer {
					Some(Initializer { maximum_required_gas, .. }) => maximum_required_gas,
					None => 0,
				};
				// total maximum gas must also include the gas used for updating the proxy before
				// the the initializer is called.
				50_000 + initializer_max_gas
			},
			Command::UnlockNativeToken { .. } => 100_000,
			Command::RegisterForeignToken { .. } => 1_200_000,
			Command::MintForeignToken { .. } => 100_000,
		}
	}
}

impl GasMeter for () {
	fn maximum_dispatch_gas_used_at_most(_: &Command) -> u64 {
		1
	}
}

// Origin for high-priority governance commands
pub fn primary_governance_origin() -> H256 {
	hex!("0000000000000000000000000000000000000000000000000000000000000001").into()
}

// Origin for lower-priority governance commands
pub fn second_governance_origin() -> H256 {
	hex!("0000000000000000000000000000000000000000000000000000000000000002").into()
}
