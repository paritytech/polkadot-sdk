#[test]
fn define_versioned_type_accepts_valid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_type/pass/*.rs";

	// Assert
	cases.pass(path);
}

#[test]
fn define_versioned_type_rejects_invalid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_type/fail/*.rs";

	// Assert
	cases.compile_fail(path);
}

#[test]
fn define_versioned_interface_rejects_invalid_macro_input() {
	// Arrange
	let cases = trybuild::TestCases::new();

	// Act
	let path = "tests/ui/define_versioned_interface/fail/*.rs";

	// Assert
	cases.compile_fail(path);
}
