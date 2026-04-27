mod inner {
	use pallet_revive_proc_macro::define_versioned_type;

	define_versioned_type! {
		pub struct UiPublicInheritedV1 {
			hidden: u8,
		}

		#[versioned_type(extend)]
		pub struct UiPublicInheritedV2 {
			pub visible: u16,
		}
	}
}

fn main() {
	let _value = inner::UiPublicInheritedV2 { hidden: 1, visible: 2 };
}
