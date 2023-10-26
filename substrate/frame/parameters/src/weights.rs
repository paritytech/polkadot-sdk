#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(clippy::unnecessary_cast)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

pub trait WeightInfo {
	fn set_parameter() -> Weight;
}

impl WeightInfo for () {
	fn set_parameter() -> Weight {
		RocksDbWeight::get().reads_writes(2, 1)
	}
}
