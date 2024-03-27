// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use frame_executive::tests::*;
use honggfuzz::fuzz;
use sp_runtime::{
	testing::{Block, Header},
	traits::Block as BlockT,
};
fn main() {
	loop {
		fuzz!(|data: (usize, usize, bool)| {
			callbacks_in_block_execution_works_inner(data.0, data.1, data.2);
		});
	}
}

fn callbacks_in_block_execution_works_inner(inx: usize, txx: usize, mbms_active: bool) {
	MbmActive::set(mbms_active);

	let mut extrinsics = Vec::new();

	let header = new_test_ext(10).execute_with(|| {
		MockedSystemCallbacks::reset();
		Executive::initialize_block(&Header::new_from_number(1));
		assert_eq!(SystemCallbacksCalled::get(), 1);

		for i in 0..inx {
			let xt = if i % 2 == 0 {
				TestXt::new(RuntimeCall::Custom(custom::Call::inherent {}), None)
			} else {
				TestXt::new(RuntimeCall::Custom2(custom2::Call::optional_inherent {}), None)
			};
			Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();
			extrinsics.push(xt);
		}

		for t in 0..txx {
			let xt = TestXt::new(
				RuntimeCall::Custom2(custom2::Call::some_call {}),
				sign_extra(1, t as u64, 0),
			);
			// Extrinsics can be applied even when MBMs are active. Only the `execute_block`
			// will reject it.
			Executive::apply_extrinsic(xt.clone()).unwrap().unwrap();
			extrinsics.push(xt);
		}

		Executive::finalize_block()
	});
	assert_eq!(SystemCallbacksCalled::get(), 3);

	new_test_ext(10).execute_with(|| {
		let header = std::panic::catch_unwind(|| {
			Executive::execute_block(Block::new(header, extrinsics));
		});

		match header {
			Err(e) => {
				let err = e.downcast::<&str>().unwrap();
				assert_eq!(*err, "Only inherents are allowed in this block");
				assert!(
					MbmActive::get() && txx > 0,
					"Transactions should be rejected when MBMs are active"
				);
			},
			Ok(_) => {
				assert_eq!(SystemCallbacksCalled::get(), 3);
				assert!(
					!MbmActive::get() || txx == 0,
					"MBMs should be deactivated after finalization"
				);
			},
		}
	});
}
