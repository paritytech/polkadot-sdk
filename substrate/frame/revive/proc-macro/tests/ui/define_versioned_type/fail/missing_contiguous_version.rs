use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiMissingVersionV1 {
		pub first: u8,
	}

	pub struct UiMissingVersionV3 {
		pub third: u8,
	}
}

fn main() {}
