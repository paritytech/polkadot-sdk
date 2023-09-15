// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! Tests.

use super::{mock::*, *};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, pallet_prelude::Pays};

/// returns CID hash of 68 bytes of given `i`.
fn create_cid(i: u8) -> OpaqueCid {
	let cid: OpaqueCid = [i; 68].to_vec().try_into().unwrap();
	cid
}

#[test]
fn set_charter_works() {
	new_test_ext().execute_with(|| {
		// wrong origin.
		let origin = RuntimeOrigin::signed(SomeAccount::get());
		let cid = create_cid(1);
		assert_noop!(CollectiveContent::set_charter(origin, cid), BadOrigin);

		// success.
		let origin = RuntimeOrigin::signed(CharterManager::get());
		let cid = create_cid(2);

		assert_ok!(CollectiveContent::set_charter(origin, cid.clone()));
		assert_eq!(Charter::<Test, _>::get(), Some(cid.clone()));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::NewCharterSet { cid }));

		// reset. success.
		let origin = RuntimeOrigin::signed(CharterManager::get());
		let cid = create_cid(3);

		assert_ok!(CollectiveContent::set_charter(origin, cid.clone()));
		assert_eq!(Charter::<Test, _>::get(), Some(cid.clone()));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::NewCharterSet { cid }));
	});
}

#[test]
fn announce_works() {
	new_test_ext().execute_with(|| {
		let now = frame_system::Pallet::<Test>::block_number();
		// wrong origin.
		let origin = RuntimeOrigin::signed(SomeAccount::get());
		let cid = create_cid(1);

		assert_noop!(CollectiveContent::announce(origin, cid, None), BadOrigin);

		// success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(2);
		let maybe_expire_at = None;

		assert_ok!(CollectiveContent::announce(origin, cid.clone(), maybe_expire_at));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementAnnounced {
			cid,
			expire_at: now.saturating_add(AnnouncementLifetime::get()),
		}));

		// one more. success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(3);
		let maybe_expire_at = None;

		assert_ok!(CollectiveContent::announce(origin, cid.clone(), maybe_expire_at));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementAnnounced {
			cid,
			expire_at: now.saturating_add(AnnouncementLifetime::get()),
		}));

		// one more with expire. success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(4);
		let maybe_expire_at = DispatchTime::<_>::After(10);

		assert_ok!(CollectiveContent::announce(origin, cid.clone(), Some(maybe_expire_at)));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementAnnounced {
			cid,
			expire_at: maybe_expire_at.evaluate(now),
		}));

		// one more with later expire. success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(5);
		let maybe_expire_at = DispatchTime::<_>::At(now + 20);

		assert_ok!(CollectiveContent::announce(origin, cid.clone(), Some(maybe_expire_at)));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementAnnounced {
			cid,
			expire_at: maybe_expire_at.evaluate(now),
		}));

		// one more with earlier expire. success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(6);
		let maybe_expire_at = DispatchTime::<_>::At(now + 5);

		assert_ok!(CollectiveContent::announce(origin, cid.clone(), Some(maybe_expire_at)));
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementAnnounced {
			cid,
			expire_at: maybe_expire_at.evaluate(now),
		}));

		// one more with earlier expire. success.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(7);
		let maybe_expire_at = DispatchTime::<_>::At(now + 5);

		assert_eq!(<Announcements<Test, _>>::count(), MaxAnnouncements::get());
		assert_noop!(
			CollectiveContent::announce(origin, cid.clone(), Some(maybe_expire_at)),
			Error::<Test>::TooManyAnnouncements
		);
	});
}

#[test]
fn remove_announcement_works() {
	new_test_ext().execute_with(|| {
		// wrong origin.
		let origin = RuntimeOrigin::signed(CharterManager::get());
		let cid = create_cid(8);

		assert_noop!(
			CollectiveContent::remove_announcement(origin, cid),
			Error::<Test>::MissingAnnouncement
		);

		// missing announcement.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(9);

		assert_noop!(
			CollectiveContent::remove_announcement(origin, cid),
			Error::<Test>::MissingAnnouncement
		);

		// wrong origin. announcement not yet expired.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(10);
		assert_ok!(CollectiveContent::announce(origin.clone(), cid.clone(), None));
		assert!(<Announcements<Test>>::contains_key(cid.clone()));

		let origin = RuntimeOrigin::signed(SomeAccount::get());
		assert_noop!(CollectiveContent::remove_announcement(origin, cid.clone()), BadOrigin);
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		assert_ok!(CollectiveContent::remove_announcement(origin, cid));

		// success.

		// remove first announcement and assert.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(11);
		assert_ok!(CollectiveContent::announce(origin.clone(), cid.clone(), None));
		assert!(<Announcements<Test>>::contains_key(cid.clone()));

		let info = CollectiveContent::remove_announcement(origin.clone(), cid.clone()).unwrap();
		assert_eq!(info.pays_fee, Pays::Yes);
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementRemoved {
			cid: cid.clone(),
		}));
		assert_noop!(
			CollectiveContent::remove_announcement(origin, cid.clone()),
			Error::<Test>::MissingAnnouncement
		);
		assert!(!<Announcements<Test>>::contains_key(cid));

		// remove expired announcement and assert.
		let origin = RuntimeOrigin::signed(AnnouncementManager::get());
		let cid = create_cid(12);
		assert_ok!(CollectiveContent::announce(
			origin.clone(),
			cid.clone(),
			Some(DispatchTime::<_>::At(10))
		));
		assert!(<Announcements<Test>>::contains_key(cid.clone()));

		System::set_block_number(11);
		let origin = RuntimeOrigin::signed(SomeAccount::get());
		let info = CollectiveContent::remove_announcement(origin.clone(), cid.clone()).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(RuntimeEvent::CollectiveContent(Event::AnnouncementRemoved {
			cid: cid.clone(),
		}));
		assert_noop!(
			CollectiveContent::remove_announcement(origin, cid),
			Error::<Test>::MissingAnnouncement
		);
	});
}
