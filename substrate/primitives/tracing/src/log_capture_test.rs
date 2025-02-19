use std::{
	io::Write,
	sync::{Arc, Mutex},
};
use tracing_subscriber::fmt::MakeWriter;

/// A reusable log capturing struct for unit tests.
/// Captures logs written during test execution for assertions.
pub struct LogCapture {
	buffer: Arc<Mutex<Vec<u8>>>,
}

impl LogCapture {
	/// Creates a new `LogCapture` instance with an internal buffer.
	pub fn new() -> Self {
		LogCapture { buffer: Arc::new(Mutex::new(Vec::new())) }
	}

	/// Retrieves the captured logs as a `String`.
	pub fn get_logs(&self) -> String {
		String::from_utf8(self.buffer.lock().unwrap().clone()).unwrap()
	}

	/// Returns a clone of the internal buffer for use in `MakeWriter`.
	pub fn writer(&self) -> Self {
		LogCapture { buffer: Arc::clone(&self.buffer) }
	}
}

impl Write for LogCapture {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		let mut logs = self.buffer.lock().unwrap();
		logs.extend_from_slice(buf);
		Ok(buf.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}

impl<'a> MakeWriter<'a> for LogCapture {
	type Writer = Self;

	/// Provides a `MakeWriter` implementation for `tracing_subscriber`.
	fn make_writer(&'a self) -> Self::Writer {
		self.writer()
	}
}

/// Runs a test block with logging enabled and captures logs for assertions.
/// Usage:
/// ```
/// use sp_tracing::{assert_logs_contain, capturing_logs};
/// #[test]
/// fn test_logging_capture() {
///     let log_capture = capturing_logs!({
/// 		tracing::info!("Test log message");
/// 	});
///
///     assert_logs_contain!(log_capture, "Test log message");
/// }
/// ```
#[macro_export]
macro_rules! capturing_logs {
	($test:block) => {{
		let log_capture = $crate::log_capture_test::LogCapture::new();
		let subscriber = tracing_subscriber::fmt().with_writer(log_capture.writer()).finish();

		tracing::subscriber::with_default(subscriber, || $test);

		log_capture
	}};
}

/// Macro to assert that captured logs contain a specific substring.
/// Usage:
/// ```ignore
/// assert_logs_contain!(log_capture, "Expected log message");
/// ```
#[macro_export]
macro_rules! assert_logs_contain {
	($log_capture:expr, $expected:expr) => {
		let logs = $log_capture.get_logs();
		assert!(
			logs.contains($expected),
			"Expected '{}' in logs, but logs were:\n{}",
			$expected,
			logs
		);
	};
}
