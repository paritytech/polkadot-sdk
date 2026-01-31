/// A simple function for testing
pub fn hello_from_a() -> &'static str {
    "Hello from crate A!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(hello_from_a(), "Hello from crate A!");
    }
}
