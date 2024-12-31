use std::{
	future::Future,
	io::{self, IsTerminal},
	time::Instant,
};

use spinners::{Spinner, Spinners};

use super::Result;

pub(super) fn with_elapsed<F, R, EndMsg>(f: F, start_msg: &str, end_msg: EndMsg) -> Result<R>
where
	F: FnOnce() -> Result<R>,
	EndMsg: FnOnce(&R) -> String,
{
	if io::stdout().is_terminal() {
		let (start, mut sp) = start(start_msg);
		let r = f()?;

		Ok(end(r, start, &mut sp, end_msg))
	} else {
		f()
	}
}

pub(super) async fn with_elapsed_async<F, Fut, R, EndMsg>(f: F, start_msg: &str, end_msg: EndMsg) -> Result<R>
where
	F: FnOnce() -> Fut,
	Fut: Future<Output = Result<R>>,
	EndMsg: FnOnce(&R) -> String,
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

fn end<T, EndMsg>(val: T, start: Instant, sp: &mut Spinner, end_msg: EndMsg) -> T
where
	EndMsg: FnOnce(&T) -> String,
{
	sp.stop_with_message(format!("✅ {} in {:.2}s", end_msg(&val), start.elapsed().as_secs_f32()));

	val
}
