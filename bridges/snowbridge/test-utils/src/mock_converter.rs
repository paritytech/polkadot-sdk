// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use codec::Encode;
use frame_support::sp_runtime::traits::MaybeConvert;
use snowbridge_core::TokenIdOf;
use sp_core::H256;
use std::{cell::RefCell, collections::HashMap};
use xcm::{
	latest::InteriorLocation,
	prelude::{Location, Reanchorable},
};
use xcm_executor::traits::ConvertLocation;

thread_local! {
	pub static IDENTIFIER_TO_LOCATION: RefCell<HashMap<H256, Location>> = RefCell::new(HashMap::new());
	pub static LOCATION_TO_IDENTIFIER: RefCell<HashMap<Vec<u8>, H256>> = RefCell::new(HashMap::new());
}

pub fn add_location_override(location: Location, ethereum: Location, bh_context: InteriorLocation) {
	let (token_id, reanchored_location) = reanchor_to_ethereum(location, ethereum, bh_context);
	IDENTIFIER_TO_LOCATION.with(|b| b.borrow_mut().insert(token_id, reanchored_location.clone()));
	LOCATION_TO_IDENTIFIER.with(|b| b.borrow_mut().insert(reanchored_location.encode(), token_id));
}

pub fn reanchor_to_ethereum(
	location: Location,
	ethereum: Location,
	bh_context: InteriorLocation,
) -> (H256, Location) {
	let mut reanchored_lol = location.clone();
	let _ = reanchored_lol.reanchor(&ethereum, &bh_context);
	let token_id = TokenIdOf::convert_location(&reanchored_lol).unwrap();
	(token_id, reanchored_lol)
}

pub struct LocationIdConvert;
impl MaybeConvert<H256, Location> for LocationIdConvert {
	fn maybe_convert(id: H256) -> Option<Location> {
		IDENTIFIER_TO_LOCATION.with(|b| b.borrow().get(&id).and_then(|l| Option::from(l.clone())))
	}
}
