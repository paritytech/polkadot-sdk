// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> pallet_message_queue::WeightInfo for WeightInfo<T> {
    fn ready_ring_knit() -> Weight {
        todo!()
    }

    fn ready_ring_unknit() -> Weight {
        todo!()
    }

    fn service_queue_base() -> Weight {
        todo!()
    }

    fn service_page_base_completion() -> Weight {
        todo!()
    }

    fn service_page_base_no_completion() -> Weight {
        todo!()
    }

    fn service_page_item() -> Weight {
        todo!()
    }

    fn bump_service_head() -> Weight {
        todo!()
    }

    fn reap_page() -> Weight {
        todo!()
    }

    fn execute_overweight_page_removed() -> Weight {
        todo!()
    }

    fn execute_overweight_page_updated() -> Weight {
        todo!()
    }
}