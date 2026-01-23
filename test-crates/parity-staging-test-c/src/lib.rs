use parity_staging_test_a::hello_from_a;
use parity_staging_test_b::hello_from_b;

/// A function that uses both crate A and B
pub fn hello_from_c() -> String {
    format!(
        "Hello from crate C! I can use A directly: '{}' and B: '{}'",
        hello_from_a(),
        hello_from_b()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = hello_from_c();
        assert!(result.contains("crate C"));
        assert!(result.contains("crate A"));
        assert!(result.contains("crate B"));
    }
}
