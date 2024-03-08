// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

/// Port from https://github.com/roberts-ivanovs/eth-encode-packed-rs
use ethabi::ethereum_types::{Address, U256};

pub struct TakeLastXBytes(pub usize);

#[allow(dead_code)]
pub enum SolidityDataType<'a> {
	String(&'a str),
	Address(Address),
	Bytes(&'a [u8]),
	Bool(bool),
	Number(U256),
	NumberWithShift(U256, TakeLastXBytes),
}

pub mod abi {
	use super::SolidityDataType;
	use sp_std::{vec, vec::Vec};

	/// Pack a single `SolidityDataType` into bytes
	#[allow(clippy::needless_lifetimes)]
	fn pack<'a>(data_type: &'a SolidityDataType) -> Vec<u8> {
		let mut res = Vec::new();
		match data_type {
			SolidityDataType::String(s) => {
				res.extend(s.as_bytes());
			},
			SolidityDataType::Address(a) => {
				res.extend(a.0);
			},
			SolidityDataType::Number(n) => {
				for b in n.0.iter().rev() {
					let bytes = b.to_be_bytes();
					res.extend(bytes);
				}
			},
			SolidityDataType::Bytes(b) => {
				res.extend(*b);
			},
			SolidityDataType::Bool(b) => {
				if *b {
					res.push(1);
				} else {
					res.push(0);
				}
			},
			SolidityDataType::NumberWithShift(n, to_take) => {
				let local_res = n.0.iter().rev().fold(vec![], |mut acc, i| {
					let bytes = i.to_be_bytes();
					acc.extend(bytes);
					acc
				});

				let to_skip = local_res.len() - (to_take.0 / 8);
				let local_res = local_res.into_iter().skip(to_skip).collect::<Vec<u8>>();
				res.extend(local_res);
			},
		};
		res
	}

	pub fn encode_packed(items: &[SolidityDataType]) -> Vec<u8> {
		let res = items.iter().fold(Vec::new(), |mut acc, i| {
			let pack = pack(i);
			acc.push(pack);
			acc
		});
		res.join(&[][..])
	}
}
