use async_trait::async_trait;

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{
		RelayerMode, SourceClient as MessageLaneSourceClient,
		TargetClient as MessageLaneTargetClient,
	},
	relay_strategy::{AltruisticStrategy, RationalStrategy, RelayReference, RelayStrategy},
};

/// The relayer doesn't care about rewards.
#[derive(Clone)]
pub struct MixStrategy {
	relayer_mode: RelayerMode,
}

impl MixStrategy {
	/// Create mix strategy instance
	pub fn new(relayer_mode: RelayerMode) -> Self {
		Self { relayer_mode }
	}
}

#[async_trait]
impl RelayStrategy for MixStrategy {
	async fn decide<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&self,
		reference: &mut RelayReference<P, SourceClient, TargetClient>,
	) -> bool {
		match self.relayer_mode {
			RelayerMode::Altruistic => AltruisticStrategy.decide(reference).await,
			RelayerMode::Rational => RationalStrategy.decide(reference).await,
		}
	}
}
