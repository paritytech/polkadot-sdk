#![allow(missing_docs)]
use serde::{Deserialize, Serialize};
use sp_core::{H160, H256, U256};

/// A tracer that can be used to debug a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tracer {
	CallTracer,
}

/// The type of call that was executed.
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum CallType {
	#[default]
	Call,
	StaticCall,
	DelegateCall,
}

/// A smart contract execution call trace.
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
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
	pub gas: U256,
	#[serde(rename = "gasUsed")]
	///  Return data.
	pub output: Option<Vec<u8>>,
	/// Amount of gas provided for the call.
	/// Amount of gas used.
	pub gas_used: U256,
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
