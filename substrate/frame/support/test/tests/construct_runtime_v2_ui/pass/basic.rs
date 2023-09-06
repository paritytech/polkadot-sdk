use frame_support::derive_impl;

pub type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

#[frame_support::construct_runtime_v2]
mod runtime {
    #[frame::runtime]
    pub struct Runtime;

    #[frame::pallets]
    #[frame::derive(RuntimeCall, RuntimeEvent, RuntimeOrigin, RuntimeError)]
    pub struct Pallets {
        #[frame::pallet_index(0)]
        System: frame_system
    }
}

fn main() {}
