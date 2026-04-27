use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub enum UiStructFailV1 {
		Variant,
	}

	#[versioned_type(extend)]
	pub struct UiStructFailV2 {
		pub field: u8,
	}
}

fn main() {}
