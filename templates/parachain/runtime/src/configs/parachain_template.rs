use crate::*;

/// Configure the pallet template in pallets/template.
impl pallet_parachain_template::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_parachain_template::weights::SubstrateWeight<Runtime>;
}

