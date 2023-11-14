#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> cumulus_pallet_parachain_system::WeightInfo for WeightInfo<T> {
	fn enqueue_inbound_downward_messages(_n: u32, ) -> Weight { todo!() }
}
