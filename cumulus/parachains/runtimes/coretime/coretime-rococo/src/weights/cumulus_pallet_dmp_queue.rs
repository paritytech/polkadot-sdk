#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> cumulus_pallet_dmp_queue::WeightInfo for WeightInfo<T> {
    fn on_idle_good_msg() -> Weight {
        todo!()
    }

    fn on_idle_large_msg() -> Weight {
        todo!()
    }

    fn on_idle_overweight_good_msg() -> Weight {
        todo!()
    }

    fn on_idle_overweight_large_msg() -> Weight {
        todo!()
    }
}