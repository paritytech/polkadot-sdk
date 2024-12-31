use std::{
	future::Future,
	io::{self, IsTerminal},
	time::Instant,
};

use spinners::{Spinner, Spinners};

use super::Result;

pub(super) fn with_elapsed<F, R>(
	f: F,
	start_msg: &str,
	end_msg: impl FnOnce(&R) -> String,
) -> Result<R>
where
	F: FnOnce() -> Result<R>,
{
	if io::stdout().is_terminal() {
		let (start, mut sp) = start(start_msg);
		let r = f()?;

		Ok(end(r, start, &mut sp, end_msg))
	} else {
		f()
	}
}

pub(super) async fn with_elapsed_async<F, Fut, R>(
	f: F,
	start_msg: &str,
	end_msg: impl FnOnce(&R) -> String,
) -> Result<R>
where
	F: FnOnce() -> Fut,
	Fut: Future<Output = Result<R>>,
{
	if io::stdout().is_terminal() {
		let (start, mut sp) = start(start_msg);
		let r = f().await?;

		Ok(end(r, start, &mut sp, end_msg))
	} else {
		f().await
	}
}

fn start(start_msg: &str) -> (Instant, Spinner) {
	let start = Instant::now();
	let sp = Spinner::new(Spinners::Dots, format!("⏳ {start_msg}"));

	(start, sp)
}

fn end<T>(val: T, start: Instant, sp: &mut Spinner, end_msg: impl FnOnce(&T) -> String) -> T {
	sp.stop_with_message(format!(
		"✅ {} in ({:.2}s)",
		end_msg(&val),
		start.elapsed().as_secs_f32()
	));

	val
}
