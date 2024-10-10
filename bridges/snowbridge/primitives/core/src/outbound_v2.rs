use codec::{Decode, Encode};
use frame_support::PalletError;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::{RuntimeDebug, H256};
pub use v2::{Command, CommandWrapper, Initializer, Message};

mod v2 {
	use crate::outbound::OperatingMode;
	use codec::{Decode, Encode};
	use ethabi::Token;
	use frame_support::{pallet_prelude::ConstU32, BoundedVec};
	use scale_info::TypeInfo;
	use sp_core::{RuntimeDebug, H160, H256, U256};
	use sp_std::{borrow::ToOwned, vec, vec::Vec};

	/// A message which can be accepted by implementations of `/[`SendMessage`\]`
	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub struct Message {
		/// Origin
		pub origin: H256,
		/// ID
		pub id: H256,
		/// Fee
		pub fee: u128,
		/// Commands
		pub commands: BoundedVec<Command, ConstU32<5>>,
	}

	/// A command which is executable by the Gateway contract on Ethereum
	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
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
		/// Transfer ether from an agent contract to a recipient account
		TransferNativeFromAgent {
			/// The agent ID
			agent_id: H256,
			/// The recipient of the ether
			recipient: H160,
			/// The amount to transfer
			amount: u128,
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
				Command::Upgrade { .. } => 1,
				Command::CreateAgent { .. } => 2,
				Command::SetOperatingMode { .. } => 5,
				Command::TransferNativeFromAgent { .. } => 6,
				Command::UnlockNativeToken { .. } => 9,
				Command::RegisterForeignToken { .. } => 10,
				Command::MintForeignToken { .. } => 11,
			}
		}

		/// ABI-encode the Command.
		pub fn abi_encode(&self) -> Vec<u8> {
			match self {
				Command::Upgrade { impl_address, impl_code_hash, initializer, .. } =>
					ethabi::encode(&[Token::Tuple(vec![
						Token::Address(*impl_address),
						Token::FixedBytes(impl_code_hash.as_bytes().to_owned()),
						initializer
							.clone()
							.map_or(Token::Bytes(vec![]), |i| Token::Bytes(i.params)),
					])]),
				Command::CreateAgent { agent_id } =>
					ethabi::encode(&[Token::Tuple(vec![Token::FixedBytes(
						agent_id.as_bytes().to_owned(),
					)])]),
				Command::SetOperatingMode { mode } =>
					ethabi::encode(&[Token::Tuple(vec![Token::Uint(U256::from((*mode) as u64))])]),
				Command::TransferNativeFromAgent { agent_id, recipient, amount } =>
					ethabi::encode(&[Token::Tuple(vec![
						Token::FixedBytes(agent_id.as_bytes().to_owned()),
						Token::Address(*recipient),
						Token::Uint(U256::from(*amount)),
					])]),
				Command::UnlockNativeToken { agent_id, token, recipient, amount } =>
					ethabi::encode(&[Token::Tuple(vec![
						Token::FixedBytes(agent_id.as_bytes().to_owned()),
						Token::Address(*token),
						Token::Address(*recipient),
						Token::Uint(U256::from(*amount)),
					])]),
				Command::RegisterForeignToken { token_id, name, symbol, decimals } =>
					ethabi::encode(&[Token::Tuple(vec![
						Token::FixedBytes(token_id.as_bytes().to_owned()),
						Token::String(name.to_owned()),
						Token::String(symbol.to_owned()),
						Token::Uint(U256::from(*decimals)),
					])]),
				Command::MintForeignToken { token_id, recipient, amount } =>
					ethabi::encode(&[Token::Tuple(vec![
						Token::FixedBytes(token_id.as_bytes().to_owned()),
						Token::Address(*recipient),
						Token::Uint(U256::from(*amount)),
					])]),
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

	#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub struct CommandWrapper {
		pub kind: u8,
		pub max_dispatch_gas: u64,
		pub command: Command,
	}

	/// ABI-encoded form for delivery to the Gateway contract on Ethereum
	impl From<CommandWrapper> for Token {
		fn from(x: CommandWrapper) -> Token {
			Token::Tuple(vec![
				Token::Uint(x.kind.into()),
				Token::Uint(x.max_dispatch_gas.into()),
				Token::Bytes(x.command.abi_encode()),
			])
		}
	}
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
	/// Fee to cover cost processing the message remotely
	pub remote: Balance,
}

impl<Balance> Fee<Balance>
where
	Balance: BaseArithmetic + Unsigned + Copy,
{
	pub fn total(&self) -> Balance {
		self.local.saturating_add(self.remote)
	}
}

impl<Balance> From<(Balance, Balance)> for Fee<Balance>
where
	Balance: BaseArithmetic + Unsigned + Copy,
{
	fn from((local, remote): (Balance, Balance)) -> Self {
		Self { local, remote }
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
	/// All the gas used for submitting a message to Ethereum, minus the cost of dispatching
	/// the command within the message
	const MAXIMUM_BASE_GAS: u64;

	/// Total gas consumed at most, including verification & dispatch
	fn maximum_gas_used_at_most(command: &Command) -> u64 {
		Self::MAXIMUM_BASE_GAS + Self::maximum_dispatch_gas_used_at_most(command)
	}

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
	// The base transaction cost, which includes:
	// 21_000 transaction cost, roughly worst case 64_000 for calldata, and 100_000
	// for message verification
	const MAXIMUM_BASE_GAS: u64 = 185_000;

	fn maximum_dispatch_gas_used_at_most(command: &Command) -> u64 {
		match command {
			Command::CreateAgent { .. } => 275_000,
			Command::TransferNativeFromAgent { .. } => 60_000,
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
	const MAXIMUM_BASE_GAS: u64 = 1;

	fn maximum_dispatch_gas_used_at_most(_: &Command) -> u64 {
		1
	}
}
