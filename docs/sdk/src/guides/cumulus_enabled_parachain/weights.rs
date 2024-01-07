use frame_support::{
	parameter_types,
	weights::{constants, RuntimeDbWeight, Weight},
};

// Block weights
#[docify::export(block_weights)]
parameter_types! {
	/// Importing a block with 0 Extrinsics.
	pub const BlockExecutionWeight: Weight =
		Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(5_000_000), 0);
}

// Extrinsic weights
#[docify::export(extrinsic_weights)]
parameter_types! {
	/// Executing a NO-OP `System::remarks` Extrinsic.
	pub const ExtrinsicBaseWeight: Weight =
		Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(125_000), 0);
}

// ParityDb weights
#[docify::export(paritydb_weights)]
parameter_types! {
	/// `ParityDB` can be enabled with a feature flag, but is still experimental. These weights
	/// are available for brave runtime engineers who may want to try this out as default.
	pub const ParityDbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 8_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		write: 50_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
	};
}

// RocksDb weights
#[docify::export(rocksdb_weights)]
parameter_types! {
	/// By default, Substrate uses `RocksDB`, so this will be the weight used throughout
	/// the runtime.
	pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 25_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		write: 100_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
	};
}
