use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct MacroStructV1 {
		pub first: u8,
	}

	#[versioned_type(extend)]
	pub struct MacroStructV2 {
		pub second: u16,
	}
}

define_versioned_type! {
	pub enum MacroEnumV1 {
		First {
			value: u8,
		},
	}

	#[versioned_type(extend)]
	pub enum MacroEnumV2 {
		Second {
			other: u16,
		},
	}
}

#[test]
fn function_like_macro_expands_struct_extensions() {
	// Arrange
	let value = MacroStructV2 { first: 1, second: 2 };

	// Act
	let observed = (value.first, value.second);

	// Assert
	assert_eq!(observed, (1, 2));
}

#[test]
fn function_like_macro_expands_enum_extensions() {
	// Arrange
	let inherited = MacroEnumV2::First { value: 1 };
	let added = MacroEnumV2::Second { other: 2 };

	// Act
	let observed = match (inherited, added) {
		(MacroEnumV2::First { value }, MacroEnumV2::Second { other }) => (value, other),
		_ => panic!("expected inherited and added variants"),
	};

	// Assert
	assert_eq!(observed, (1, 2));
}
