#[frame_support::construct_runtime_v2]
mod runtime {
    #[frame::runtime]
    pub struct Runtime;

    #[frame::pallets]
    pub struct Pallets {}
}

fn main() {}
