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

use std::collections::HashSet;

use crate::{
	id_sequence::SeqID,
	pubsub::{Dispatch, Subscribe, Unsubscribe},
};

/// The shared structure to keep track on subscribers.
#[derive(Debug, Default)]
pub(super) struct Registry {
	pub(super) subscribers: HashSet<SeqID>,
}

impl Subscribe<()> for Registry {
	fn subscribe(&mut self, _subs_key: (), subs_id: SeqID) {
		self.subscribers.insert(subs_id);
	}
}
impl Unsubscribe for Registry {
	fn unsubscribe(&mut self, subs_id: SeqID) {
		self.subscribers.remove(&subs_id);
	}
}

impl<MakePayload, Payload, Error> Dispatch<MakePayload> for Registry
where
	MakePayload: FnOnce() -> Result<Payload, Error>,
	Payload: Clone,
{
	type Item = Payload;
	type Ret = Result<(), Error>;

	fn dispatch<F>(&mut self, make_payload: MakePayload, mut dispatch: F) -> Self::Ret
	where
		F: FnMut(&SeqID, Self::Item),
	{
		if !self.subscribers.is_empty() {
			let payload = make_payload()?;
			for subs_id in &self.subscribers {
				dispatch(subs_id, payload.clone());
			}
		}
		Ok(())
	}
}
