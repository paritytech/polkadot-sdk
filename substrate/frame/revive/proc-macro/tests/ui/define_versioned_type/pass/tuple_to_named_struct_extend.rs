use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiTupleNamedV1(pub u8, pub u16);

	#[versioned_type(extend)]
	pub struct UiTupleNamedV2 {
		pub third: u32,
	}
}

fn main() {
	let _value = UiTupleNamedV2 { field_0: 1, field_1: 2, third: 3 };
}
