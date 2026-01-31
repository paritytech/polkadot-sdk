use parity_staging_test_a::hello_from_a;

/// A function that uses crate A
pub fn hello_from_b() -> String {
    format!("Hello from crate B! Also, {}", hello_from_a())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert!(hello_from_b().contains("crate B"));
        assert!(hello_from_b().contains("crate A"));
    }
}
