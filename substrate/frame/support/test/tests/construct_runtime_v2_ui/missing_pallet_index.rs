#[frame_support::construct_runtime_v2]
mod runtime {
    #[frame::runtime]
    pub struct Runtime;

    #[frame::pallets]
    #[frame::derive(RuntimeCall)]
    pub struct Pallets {
        System: frame_system
    }
}

fn main() {}
