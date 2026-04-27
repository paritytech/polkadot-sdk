use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiGenericV1<'a, T, const N: usize>
	where
		T: Copy,
	{
		pub items: &'a [T; N],
	}

	#[versioned_type(extend)]
	pub struct UiGenericV2<'a, T, const N: usize>
	where
		T: Copy,
	{
		pub extra: T,
	}
}

fn main() {
	let items = [1u8; 2];
	let _value = UiGenericV2 { items: &items, extra: 3u8 };
}
