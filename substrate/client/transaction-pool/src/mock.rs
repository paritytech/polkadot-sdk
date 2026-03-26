/// TX replacement is disabled by default; enable it for tests that rely on replacement.
/// We intentionally do NOT remove the var on Drop to avoid races with parallel tests.
pub struct EnvGuard;

impl EnvGuard {
	pub fn allow_tx_replacement() -> Self {
		std::env::set_var("SUBSTRATE_ALLOW_TX_REPLACEMENT", "1");
		Self
	}
}
