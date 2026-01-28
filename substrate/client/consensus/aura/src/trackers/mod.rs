mod authorities_tracker;
mod slot_duration_tracker;

use std::{fmt::Debug, sync::Arc};

pub use authorities_tracker::AuthoritiesTracker;
use codec::Codec;
pub use slot_duration_tracker::{SlotDurationImport, SlotDurationTracker};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_runtime::traits::{Block, NumberFor};

use crate::{AuthorityId, CompatibilityMode};

/// Tracker types for AURA.
pub struct AuraTrackers<P: Pair, B: Block, C> {
	/// Authorities tracker.
	pub authorities_tracker: Arc<AuthoritiesTracker<P, B, C>>,
	/// Slot duration tracker.
	pub slot_duration_tracker: Arc<SlotDurationTracker<P, B, C>>,
}

impl<P: Pair, B: Block, C> AuraTrackers<P, B, C>
where
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	pub(crate) fn new(
		client: Arc<C>,
		compatibility_mode: &CompatibilityMode<NumberFor<B>>,
	) -> Result<Self, String> {
		Ok(Self {
			authorities_tracker: AuthoritiesTracker::new(client.clone(), compatibility_mode)?
				.into(),
			slot_duration_tracker: SlotDurationTracker::new(client)?.into(),
		})
	}

	pub(crate) fn new_empty(client: Arc<C>) -> Self {
		Self {
			authorities_tracker: AuthoritiesTracker::new_empty(client.clone()).into(),
			slot_duration_tracker: SlotDurationTracker::new_empty(client).into(),
		}
	}
}

impl<P: Pair, B: Block, C> Clone for AuraTrackers<P, B, C> {
	fn clone(&self) -> Self {
		Self {
			authorities_tracker: self.authorities_tracker.clone(),
			slot_duration_tracker: self.slot_duration_tracker.clone(),
		}
	}
}
