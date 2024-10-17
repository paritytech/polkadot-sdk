#![allow(missing_docs)]
use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{H160, H256, U256};

/// A tracer that can be used to debug a transaction.
#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum Tracer {
	CallTracer,
}

/// The type of call that was executed.
#[derive(
	Default, TypeInfo, Encode, Decode, Serialize, Deserialize, Eq, PartialEq, Clone, Debug,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum CallType {
	#[default]
	Call,
	StaticCall,
	DelegateCall,
}

/// A smart contract execution call trace.
#[derive(
	TypeInfo, Default, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
pub struct CallTrace {
	/// Address of the sender.
	pub from: H160,
	/// Address of the receiver.
	pub to: H160,
	/// Call input data.
	pub input: Vec<u8>,
	/// Amount of value transferred.
	#[serde(skip_serializing_if = "U256::is_zero")]
	pub value: U256,
	/// Type of call.
	#[serde(rename = "type")]
	pub call_type: CallType,
	/// Gas limit.
	pub gas: U256,
	/// Amount of gas used.
	#[serde(rename = "gasUsed")]
	pub gas_used: U256,
	///  Return data.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub output: Vec<u8>,
	/// Amount of gas provided for the call.
	/// List of sub-calls.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub calls: Vec<CallTrace>,
}

/// A transaction trace
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionTrace {
	#[serde(rename = "txHash")]
	pub tx_hash: H256,
	pub result: CallTrace,
}
