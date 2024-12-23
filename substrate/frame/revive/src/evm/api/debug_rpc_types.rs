#![allow(missing_docs)]
use crate::Weight;
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

/// The traces of a transaction.
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Traces<Gas = Weight> {
	CallTraces(Vec<CallTrace<Gas>>),
}

impl<Gas> Traces<Gas> {
	/// Return mapped traces with the given gas mapper.
	pub fn map<F, T>(self, gas_mapper: F) -> Traces<T>
	where
		F: Fn(Gas) -> T + Copy,
	{
		match self {
			Traces::CallTraces(traces) =>
				Traces::CallTraces(traces.into_iter().map(|trace| trace.map(gas_mapper)).collect()),
		}
	}
}

/// A smart contract execution call trace.
#[derive(
	TypeInfo, Default, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
pub struct CallTrace<Gas = Weight> {
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
	pub gas: Gas,
	/// Amount of gas used.
	#[serde(rename = "gasUsed")]
	pub gas_used: Gas,
	///  Return data.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub output: Vec<u8>,
	/// Amount of gas provided for the call.
	/// List of sub-calls.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub calls: Vec<CallTrace<Gas>>,
}

impl<Gas> CallTrace<Gas> {
	/// Return a new call gas with a mapped gas value.
	pub fn map<F, T>(self, gas_mapper: F) -> CallTrace<T>
	where
		F: Fn(Gas) -> T + Copy,
	{
		CallTrace {
			from: self.from,
			to: self.to,
			input: self.input,
			value: self.value,
			call_type: self.call_type,
			gas: gas_mapper(self.gas),
			gas_used: gas_mapper(self.gas_used),
			output: self.output,
			calls: self.calls.into_iter().map(|call| call.map(gas_mapper)).collect(),
		}
	}
}

/// A transaction trace
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionTrace {
	#[serde(rename = "txHash")]
	pub tx_hash: H256,
	pub result: CallTrace,
}
