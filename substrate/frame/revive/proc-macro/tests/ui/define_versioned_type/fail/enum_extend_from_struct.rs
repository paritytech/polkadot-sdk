use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiFailV1 {
		pub field: u8,
	}

	#[versioned_type(extend)]
	pub enum UiFailV2 {
		Variant,
	}
}

fn main() {}
