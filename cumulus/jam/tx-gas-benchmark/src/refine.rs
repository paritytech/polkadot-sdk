// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Running `jam_validate_block` the way the Parachain Service's refine runs it.
//!
//! The runtime blob runs as an inner PVM: code-only module, memory laid out as the service's
//! `pvf::pvm::run` lays it out, child host calls answered as the service's
//! `pvf::executor::ExecutorState::dispatch` answers them, and gas metered with the polkavm version
//! and cost model of the refine node. The gas reported is the inner PVM's, the `invoke` part of a
//! refine; the service's own work around it is not included.

use jam_types::{FetchKind, PAGE_SIZE};
use parachain_service_core::host_call::HostCall;
use polkavm::{
	program::InstructionSetKind, ArcBytes, BackendKind, CacheModel, Config, CostModelKind, Engine,
	GasMeteringKind, InterruptKind, MemoryMap, MemoryMapBuilder, MemoryProtection, Module,
	ModuleConfig, ProgramBlob, ProgramCounter, ProgramParts, RawInstance, Reg,
};

/// The export the service invokes.
const PVF_ENTRY_POINT: &str = "jam_validate_block";
/// The service's cap on the inner PVM's heap (`service/src/constants.rs`).
const MAX_PVF_HEAP_SIZE: u64 = 1 << 30;
/// What JAM returns for an item that does not exist.
const ABSENT: u64 = u64::MAX;
/// The gas the inner PVM gets: the Gray Paper's refine gas limit.
const GAS_LIMIT: i64 = 5_000_000_000;

/// A runtime blob split into what the service sets up before invoking it.
pub struct Pvf {
	code: ArcBytes,
	ro_data: ArcBytes,
	rw_data: ArcBytes,
	memory: MemoryMap,
	entry_pc: u32,
}

impl Pvf {
	pub fn parse(blob: &[u8]) -> Result<Self, String> {
		let parts = ProgramParts::from_bytes(ArcBytes::from(blob))
			.map_err(|error| format!("the runtime is no PolkaVM blob: {error}"))?;
		let (code, ro_data, rw_data) =
			(parts.code_and_jump_table.clone(), parts.ro_data.clone(), parts.rw_data.clone());
		let memory = MemoryMapBuilder::new(PAGE_SIZE)
			.ro_data_size(parts.ro_data_size)
			.rw_data_size(parts.rw_data_size)
			.stack_size(parts.stack_size)
			.build()
			.map_err(|error| format!("invalid runtime memory map: {error}"))?;
		let program = ProgramBlob::from_parts(parts)
			.map_err(|error| format!("invalid runtime program: {error}"))?;
		let entry_pc = program
			.exports()
			.find(|export| export == PVF_ENTRY_POINT)
			.ok_or_else(|| format!("the runtime does not export `{PVF_ENTRY_POINT}`"))?
			.program_counter()
			.0;
		Ok(Pvf { code, ro_data, rw_data, memory, entry_pc })
	}
}

/// What a refine of one PoV produced.
pub struct Outcome {
	/// Gas the inner PVM consumed.
	pub gas: i64,
	pub host_calls: u64,
	/// From `set_parent_head_hash`.
	pub parent_head_hash: Option<[u8; 32]>,
	/// From `set_head`.
	pub head: Option<Vec<u8>>,
	pub upward_messages: usize,
}

/// Run `jam_validate_block` on `pov` (work-item extrinsic 0) under `refine_context` (JAM encoded).
pub fn run(pvf: &Pvf, pov: &[u8], refine_context: &[u8]) -> Result<Outcome, String> {
	let mut config = Config::new();
	config.set_backend(Some(BackendKind::Interpreter));
	config.set_allow_dynamic_paging(true);
	config.set_default_cost_model(Some(CostModelKind::Full(CacheModel::L2Hit)));
	let engine = Engine::new(&config).map_err(|error| format!("polkavm engine: {error}"))?;

	// The service's `machine` host call spawns the inner PVM from the code alone.
	let mut parts = ProgramParts::empty(InstructionSetKind::JamV1);
	parts.code_and_jump_table = pvf.code.clone();
	let blob = ProgramBlob::from_parts(parts).map_err(|error| format!("inner PVM: {error}"))?;
	let mut module_config = ModuleConfig::new();
	module_config.set_gas_metering(Some(GasMeteringKind::Sync));
	module_config.set_dynamic_paging(true);
	let module = Module::from_blob(&engine, &module_config, blob)
		.map_err(|error| format!("inner PVM module: {error}"))?;
	let mut instance =
		module.instantiate().map_err(|error| format!("inner PVM instance: {error}"))?;
	instance.set_next_program_counter(ProgramCounter(pvf.entry_pc));

	let memory = &pvf.memory;
	map(&mut instance, memory.ro_data_address(), memory.ro_data_size(), &pvf.ro_data)?;
	map(&mut instance, memory.rw_data_address(), memory.rw_data_size(), &pvf.rw_data)?;
	map(&mut instance, memory.stack_address_low(), memory.stack_size(), &[])?;
	instance.set_reg(Reg::SP, u64::from(memory.stack_address_high()));
	instance.set_reg(Reg::RA, u64::from(polkavm::RETURN_TO_HOST));
	instance.set_gas(GAS_LIMIT);

	let mut host = Host {
		pov,
		refine_context,
		heap: Heap::new(memory),
		host_calls: 0,
		exports: 0,
		parent_head_hash: None,
		head: None,
		upward_messages: 0,
		logs: Vec::new(),
	};
	loop {
		let interrupt = instance.run().map_err(|error| format!("inner PVM run: {error}"))?;
		let abort = match interrupt {
			InterruptKind::Finished => break,
			InterruptKind::Ecalli(index) => match host.dispatch(&mut instance, index) {
				Ok(()) => continue,
				Err(error) => error,
			},
			InterruptKind::Trap => "the runtime panicked".into(),
			InterruptKind::NotEnoughGas => "the runtime ran out of gas".into(),
			InterruptKind::Segfault(fault) => {
				format!("the runtime faulted at page {:#x}", fault.page_address)
			},
			InterruptKind::Step => unreachable!("step tracing is not enabled; qed"),
		};
		return Err(format!(
			"{abort} at {:?} after {} gas; runtime log:\n{}",
			instance.program_counter(),
			GAS_LIMIT - instance.gas(),
			host.logs.join("\n"),
		));
	}

	Ok(Outcome {
		gas: GAS_LIMIT - instance.gas(),
		host_calls: host.host_calls,
		parent_head_hash: host.parent_head_hash,
		head: host.head,
		upward_messages: host.upward_messages,
	})
}

/// Map and zero the pages spanning `[address, address + len)` and write `data` at their start.
fn map(instance: &mut RawInstance, address: u32, len: u32, data: &[u8]) -> Result<(), String> {
	if len == 0 {
		return Ok(());
	}
	instance
		.zero_memory_with_memory_protection(
			address,
			len.div_ceil(PAGE_SIZE) * PAGE_SIZE,
			MemoryProtection::ReadWrite,
		)
		.map_err(|error| format!("mapping {address:#x}: {error}"))?;
	instance
		.write_memory(address, data)
		.map_err(|error| format!("writing {address:#x}: {error}"))
}

/// The inner PVM's heap as the service tracks it for `grow_heap`.
struct Heap {
	page_size: u64,
	top: u64,
	mapped_until: u64,
	limit: u64,
}

impl Heap {
	fn new(memory: &MemoryMap) -> Self {
		Heap {
			page_size: u64::from(memory.page_size()),
			top: u64::from(memory.heap_base()),
			mapped_until: u64::from(memory.rw_data_address()) + u64::from(memory.rw_data_size()),
			limit: u64::from(memory.heap_base()) +
				u64::from(memory.max_heap_size()).min(MAX_PVF_HEAP_SIZE),
		}
	}
}

/// The service's side of the child host calls.
struct Host<'a> {
	pov: &'a [u8],
	refine_context: &'a [u8],
	heap: Heap,
	host_calls: u64,
	exports: u64,
	parent_head_hash: Option<[u8; 32]>,
	head: Option<Vec<u8>>,
	upward_messages: usize,
	logs: Vec<String>,
}

impl Host<'_> {
	fn dispatch(&mut self, instance: &mut RawInstance, index: u32) -> Result<(), String> {
		self.host_calls += 1;
		let call = HostCall::try_from(u64::from(index))
			.map_err(|_| format!("the runtime called the unknown host call {index}"))?;
		let a = [Reg::A0, Reg::A1, Reg::A2, Reg::A3, Reg::A4, Reg::A5].map(|reg| instance.reg(reg));
		match call {
			HostCall::Gas => instance.set_reg(Reg::A0, instance.gas().max(0) as u64),
			HostCall::GrowHeap => {
				let pages = a[0];
				let heap = &mut self.heap;
				let top_pages = heap.top.div_ceil(heap.page_size);
				if pages <= top_pages || pages > heap.limit / heap.page_size {
					instance.set_reg(Reg::A0, top_pages);
					return Ok(());
				}
				let new_top = pages * heap.page_size;
				let end = new_top.div_ceil(heap.page_size) * heap.page_size;
				if end > heap.mapped_until {
					instance
						.zero_memory_with_memory_protection(
							heap.mapped_until as u32,
							(end - heap.mapped_until) as u32,
							MemoryProtection::ReadWrite,
						)
						.map_err(|error| format!("growing the heap: {error}"))?;
					heap.mapped_until = end;
				}
				heap.top = new_top;
				instance.set_reg(Reg::A0, pages);
			},
			HostCall::Fetch => {
				let (out, offset, cap) = (a[0], a[1], a[2]);
				let item = match (FetchKind::try_from(a[3]), a[4]) {
					(Ok(FetchKind::OurExtrinsic), 0) => Some(self.pov),
					(Ok(FetchKind::RefineContext), _) => Some(self.refine_context),
					_ => None,
				};
				let Some(item) = item else {
					instance.set_reg(Reg::A0, ABSENT);
					return Ok(());
				};
				let full = item.len() as u64;
				// JAM writes the item from `offset` into a `cap` sized buffer, the service copies
				// `min(full, cap)` bytes of that buffer into the child.
				let mut buffer = vec![0u8; cap as usize];
				let from = offset.min(full) as usize;
				let n = (full - from as u64).min(cap) as usize;
				buffer[..n].copy_from_slice(&item[from..from + n]);
				let len = full.min(cap) as usize;
				if len > 0 {
					write(instance, out, &buffer[..len])?;
				}
				instance.set_reg(Reg::A0, full);
			},
			HostCall::HistoricalLookup => instance.set_reg(Reg::A0, ABSENT),
			HostCall::Export => {
				instance.set_reg(Reg::A0, self.exports);
				self.exports += 1;
			},
			HostCall::SetParentHeadHash => {
				let hash = read(instance, a[0], 32)?;
				if self
					.parent_head_hash
					.replace(hash.try_into().expect("read 32 bytes; qed"))
					.is_some()
				{
					return Err("the runtime declared its parent head twice".into());
				}
			},
			HostCall::SetHead => {
				let head = read(instance, a[0], a[1])?;
				if self.head.replace(head).is_some() {
					return Err("the runtime declared its head twice".into());
				}
			},
			HostCall::SendUpwardMessage => self.upward_messages += 1,
			HostCall::ReportError => {
				let payload = read(instance, a[0], a[1])?;
				return Err(format!(
					"the runtime reported `{}`",
					String::from_utf8_lossy(&payload)
				));
			},
			HostCall::JamLog | HostCall::Log => {
				let target = read(instance, a[1], a[2])?;
				let text = read(instance, a[3], a[4])?;
				self.logs.push(format!(
					"[{}] {}",
					String::from_utf8_lossy(&target),
					String::from_utf8_lossy(&text)
				));
			},
		}
		Ok(())
	}
}

fn read(instance: &mut RawInstance, address: u64, len: u64) -> Result<Vec<u8>, String> {
	if len == 0 {
		return Ok(Vec::new());
	}
	instance
		.read_memory(address as u32, len as u32)
		.map_err(|error| format!("reading runtime memory at {address:#x}: {error}"))
}

fn write(instance: &mut RawInstance, address: u64, data: &[u8]) -> Result<(), String> {
	instance
		.write_memory(address as u32, data)
		.map_err(|error| format!("writing runtime memory at {address:#x}: {error}"))
}
