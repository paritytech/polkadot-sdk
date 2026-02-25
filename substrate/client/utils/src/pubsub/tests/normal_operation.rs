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

use super::*;

#[test]
fn positive_rx_receives_relevant_messages_and_terminates_upon_hub_drop() {
	block_on(async {
		let hub = TestHub::new(TK);
		assert_eq!(hub.subs_count(), 0);

		// No subscribers yet. That message is not supposed to get to anyone.
		hub.send(0);

		let mut rx_01 = hub.subscribe(SubsKey::new(), 100_000);
		assert_eq!(hub.subs_count(), 1);

		// That message is sent after subscription. Should be delivered into rx_01.
		hub.send(1);
		assert_eq!(Some(1), rx_01.next().await);

		// Hub is disposed. The rx_01 should be over after that.
		std::mem::drop(hub);

		assert!(rx_01.is_terminated());
		assert_eq!(None, rx_01.next().await);
	});
}

#[test]
fn positive_subs_count_is_correct_upon_drop_of_rxs() {
	block_on(async {
		let hub = TestHub::new(TK);
		assert_eq!(hub.subs_count(), 0);

		let rx_01 = hub.subscribe(SubsKey::new(), 100_000);
		assert_eq!(hub.subs_count(), 1);
		let rx_02 = hub.subscribe(SubsKey::new(), 100_000);
		assert_eq!(hub.subs_count(), 2);

		std::mem::drop(rx_01);
		assert_eq!(hub.subs_count(), 1);
		std::mem::drop(rx_02);
		assert_eq!(hub.subs_count(), 0);
	});
}

#[test]
fn positive_subs_count_is_correct_upon_drop_of_rxs_on_cloned_hubs() {
	block_on(async {
		let hub_01 = TestHub::new(TK);
		let hub_02 = hub_01.clone();
		assert_eq!(hub_01.subs_count(), 0);
		assert_eq!(hub_02.subs_count(), 0);

		let rx_01 = hub_02.subscribe(SubsKey::new(), 100_000);
		assert_eq!(hub_01.subs_count(), 1);
		assert_eq!(hub_02.subs_count(), 1);

		let rx_02 = hub_02.subscribe(SubsKey::new(), 100_000);
		assert_eq!(hub_01.subs_count(), 2);
		assert_eq!(hub_02.subs_count(), 2);

		std::mem::drop(rx_01);
		assert_eq!(hub_01.subs_count(), 1);
		assert_eq!(hub_02.subs_count(), 1);

		std::mem::drop(rx_02);
		assert_eq!(hub_01.subs_count(), 0);
		assert_eq!(hub_02.subs_count(), 0);
	});
}
