use paste::paste;

use futures::{channel::oneshot, select, Future, FutureExt};
use polkadot_node_subsystem::{
	overseer, AllMessages, FromOrchestra, HeadSupportsParachains, Overseer, OverseerConnector,
	OverseerHandle, SpawnGlue, SpawnedSubsystem, Subsystem, SubsystemError,
};
use std::time::Duration;
use tokio::time::sleep;
macro_rules! mock {
	// Just query by relay parent
	($subsystem_name:ident) => {
		paste! {
			pub struct [<Mock $subsystem_name >] {}
			#[overseer::subsystem($subsystem_name, error=SubsystemError, prefix=self::overseer)]
			impl<Context> [<Mock $subsystem_name >] {
				fn start(self, ctx: Context) -> SpawnedSubsystem {
					let future = self.run(ctx).map(|_| Ok(())).boxed();

					SpawnedSubsystem { name: stringify!($subsystem_name), future }
				}
			}

			#[overseer::contextbounds($subsystem_name, prefix = self::overseer)]
			impl [<Mock $subsystem_name >] {
				async fn run<Context>(self, mut ctx: Context) {
					let mut count_total_msg = 0;
					loop {
						futures::select!{
						_msg = ctx.recv().fuse() => {
							count_total_msg  +=1;
						}
						_ = sleep(Duration::from_secs(6)).fuse() => {
                            if count_total_msg > 0 {
							    gum::info!(target: "mock-subsystems", "Subsystem {} processed {} messages since last time", stringify!($subsystem_name), count_total_msg);
                            }
                            count_total_msg = 0;
						}
						}
					}
				}
			}
		}
	};
}

mock!(AvailabilityStore);
mock!(StatementDistribution);
mock!(BitfieldSigning);
mock!(BitfieldDistribution);
mock!(Provisioner);
mock!(NetworkBridgeRx);
mock!(CollationGeneration);
mock!(CollatorProtocol);
mock!(GossipSupport);
mock!(DisputeDistribution);
mock!(DisputeCoordinator);
mock!(ProspectiveParachains);
mock!(PvfChecker);
mock!(CandidateBacking);
mock!(AvailabilityDistribution);
mock!(CandidateValidation);
mock!(AvailabilityRecovery);
mock!(NetworkBridgeTx);
