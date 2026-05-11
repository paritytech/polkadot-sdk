//! Weight information for the pallet.
//!
//! The placeholder `()` impl returns `Weight::MAX` so a runtime that picks it
//! up by mistake fails loudly. Production deployments must replace it with
//! benchmarked weights from `benchmarking.rs`.

use frame::prelude::*;

pub trait WeightInfo {
	/// `reprioritize` weight after a hint-repair walk of `repair_steps` steps. The
	/// benchmark is parametric over `repair_steps`, so this yields a linear
	/// formula. The dispatchable charges
	/// `reprioritize(MaxHintRepairSteps)` up front and refunds the unused portion via
	/// `PostDispatchInfo::actual_weight`.
	fn reprioritize(repair_steps: u32) -> Weight;
}

impl WeightInfo for () {
	fn reprioritize(_repair_steps: u32) -> Weight {
		Weight::MAX
	}
}
