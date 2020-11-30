// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

//! This module takes care of loading, checking and preprocessing of a
//! wasm module before execution. It also extracts some essential information
//! from a module.

use crate::wasm::env_def::ImportSatisfyCheck;
use crate::wasm::PrefabWasmModule;
use crate::{Schedule, Config};

use parity_wasm::elements::{self, Internal, External, MemoryType, Type, ValueType};
use pwasm_utils;
use sp_std::prelude::*;

/// Currently, all imported functions must be located inside this module. We might support
/// additional modules for versioning later.
pub const IMPORT_MODULE_FN: &str = "seal0";

/// Imported memory must be located inside this module. The reason for that is that current
/// compiler toolchains might not support specifying other modules than "env" for memory imports.
pub const IMPORT_MODULE_MEMORY: &str = "env";

struct ContractModule<'a, T: Config> {
	/// A deserialized module. The module is valid (this is Guaranteed by `new` method).
	module: elements::Module,
	schedule: &'a Schedule<T>,
}

impl<'a, T: Config> ContractModule<'a, T> {
	/// Creates a new instance of `ContractModule`.
	///
	/// Returns `Err` if the `original_code` couldn't be decoded or
	/// if it contains an invalid module.
	fn new(
		original_code: &[u8],
		schedule: &'a Schedule<T>,
	) -> Result<Self, &'static str> {
		use wasmi_validation::{validate_module, PlainValidator};

		let module =
			elements::deserialize_buffer(original_code).map_err(|_| "Can't decode wasm code")?;

		// Make sure that the module is valid.
		validate_module::<PlainValidator>(&module).map_err(|_| "Module is not valid")?;

		// Return a `ContractModule` instance with
		// __valid__ module.
		Ok(ContractModule {
			module,
			schedule,
		})
	}

	/// Ensures that module doesn't declare internal memories.
	///
	/// In this runtime we only allow wasm module to import memory from the environment.
	/// Memory section contains declarations of internal linear memories, so if we find one
	/// we reject such a module.
	fn ensure_no_internal_memory(&self) -> Result<(), &'static str> {
		if self.module
			.memory_section()
			.map_or(false, |ms| ms.entries().len() > 0)
		{
			return Err("module declares internal memory");
		}
		Ok(())
	}

	/// Ensures that tables declared in the module are not too big.
	fn ensure_table_size_limit(&self, limit: u32) -> Result<(), &'static str> {
		if let Some(table_section) = self.module.table_section() {
			// In Wasm MVP spec, there may be at most one table declared. Double check this
			// explicitly just in case the Wasm version changes.
			if table_section.entries().len() > 1 {
				return Err("multiple tables declared");
			}
			if let Some(table_type) = table_section.entries().first() {
				// Check the table's initial size as there is no instruction or environment function
				// capable of growing the table.
				if table_type.limits().initial() > limit {
					return Err("table exceeds maximum size allowed")
				}
			}
		}
		Ok(())
	}

	/// Ensure that any `br_table` instruction adheres to its immediate value limit.
	fn ensure_br_table_size_limit(&self, limit: u32) -> Result<(), &'static str> {
		let code_section = if let Some(type_section) = self.module.code_section() {
			type_section
		} else {
			return Ok(());
		};
		for instr in code_section.bodies().iter().flat_map(|body| body.code().elements()) {
			use parity_wasm::elements::Instruction::BrTable;
			if let BrTable(table) = instr {
				if table.table.len() > limit as usize {
					return Err("BrTable's immediate value is too big.")
				}
			}
		}
		Ok(())
	}

	fn ensure_global_variable_limit(&self, limit: u32) -> Result<(), &'static str> {
		if let Some(global_section) = self.module.global_section() {
			if global_section.entries().len() > limit as usize {
				return Err("module declares too many globals")
			}
		}
		Ok(())
	}

	/// Ensures that no floating point types are in use.
	fn ensure_no_floating_types(&self) -> Result<(), &'static str> {
		if let Some(global_section) = self.module.global_section() {
			for global in global_section.entries() {
				match global.global_type().content_type() {
					ValueType::F32 | ValueType::F64 =>
						return Err("use of floating point type in globals is forbidden"),
					_ => {}
				}
			}
		}

		if let Some(code_section) = self.module.code_section() {
			for func_body in code_section.bodies() {
				for local in func_body.locals() {
					match local.value_type() {
						ValueType::F32 | ValueType::F64 =>
							return Err("use of floating point type in locals is forbidden"),
						_ => {}
					}
				}
			}
		}

		if let Some(type_section) = self.module.type_section() {
			for wasm_type in type_section.types() {
				match wasm_type {
					Type::Function(func_type) => {
						let return_type = func_type.return_type();
						for value_type in func_type.params().iter().chain(return_type.iter()) {
							match value_type {
								ValueType::F32 | ValueType::F64 =>
									return Err("use of floating point type in function types is forbidden"),
								_ => {}
							}
						}
					}
				}
			}
		}

		Ok(())
	}

	/// Ensure that no function exists that has more parameters than allowed.
	fn ensure_parameter_limit(&self, limit: u32) -> Result<(), &'static str> {
		let type_section = if let Some(type_section) = self.module.type_section() {
			type_section
		} else {
			return Ok(());
		};

		for Type::Function(func) in type_section.types() {
			if func.params().len() > limit as usize {
				return Err("Use of a function type with too many parameters.");
			}
		}

		Ok(())
	}

	fn inject_gas_metering(self) -> Result<Self, &'static str> {
		let gas_rules = self.schedule.rules(&self.module);
		let contract_module = pwasm_utils::inject_gas_counter(
			self.module,
			&gas_rules,
			IMPORT_MODULE_FN
		).map_err(|_| "gas instrumentation failed")?;
		Ok(ContractModule {
			module: contract_module,
			schedule: self.schedule,
		})
	}

	fn inject_stack_height_metering(self) -> Result<Self, &'static str> {
		let contract_module =
			pwasm_utils::stack_height
				::inject_limiter(self.module, self.schedule.limits.stack_height)
				.map_err(|_| "stack height instrumentation failed")?;
		Ok(ContractModule {
			module: contract_module,
			schedule: self.schedule,
		})
	}

	/// Check that the module has required exported functions. For now
	/// these are just entrypoints:
	///
	/// - 'call'
	/// - 'deploy'
	///
	/// Any other exports are not allowed.
	fn scan_exports(&self) -> Result<(), &'static str> {
		let mut deploy_found = false;
		let mut call_found = false;

		let module = &self.module;

		let types = module.type_section().map(|ts| ts.types()).unwrap_or(&[]);
		let export_entries = module
			.export_section()
			.map(|is| is.entries())
			.unwrap_or(&[]);
		let func_entries = module
			.function_section()
			.map(|fs| fs.entries())
			.unwrap_or(&[]);

		// Function index space consists of imported function following by
		// declared functions. Calculate the total number of imported functions so
		// we can use it to convert indexes from function space to declared function space.
		let fn_space_offset = module
			.import_section()
			.map(|is| is.entries())
			.unwrap_or(&[])
			.iter()
			.filter(|entry| {
				match *entry.external() {
					External::Function(_) => true,
					_ => false,
				}
			})
			.count();

		for export in export_entries {
			match export.field() {
				"call" => call_found = true,
				"deploy" => deploy_found = true,
				_ => return Err("unknown export: expecting only deploy and call functions"),
			}

			// Then check the export kind. "call" and "deploy" are
			// functions.
			let fn_idx = match export.internal() {
				Internal::Function(ref fn_idx) => *fn_idx,
				_ => return Err("expected a function"),
			};

			// convert index from function index space to declared index space.
			let fn_idx = match fn_idx.checked_sub(fn_space_offset as u32) {
				Some(fn_idx) => fn_idx,
				None => {
					// Underflow here means fn_idx points to imported function which we don't allow!
					return Err("entry point points to an imported function");
				}
			};

			// Then check the signature.
			// Both "call" and "deploy" has a () -> () function type.
			let func_ty_idx = func_entries.get(fn_idx as usize)
				.ok_or_else(|| "export refers to non-existent function")?
				.type_ref();
			let Type::Function(ref func_ty) = types
				.get(func_ty_idx as usize)
				.ok_or_else(|| "function has a non-existent type")?;
			if !func_ty.params().is_empty() ||
				!(func_ty.return_type().is_none() ||
					func_ty.return_type() == Some(ValueType::I32)) {
				return Err("entry point has wrong signature");
			}
		}

		if !deploy_found {
			return Err("deploy function isn't exported");
		}
		if !call_found {
			return Err("call function isn't exported");
		}

		Ok(())
	}

	/// Scan an import section if any.
	///
	/// This accomplishes two tasks:
	///
	/// - checks any imported function against defined host functions set, incl.
	///   their signatures.
	/// - if there is a memory import, returns it's descriptor
	/// `import_fn_banlist`: list of function names that are disallowed to be imported
	fn scan_imports<C: ImportSatisfyCheck>(&self, import_fn_banlist: &[&[u8]])
		-> Result<Option<&MemoryType>, &'static str>
	{
		let module = &self.module;

		let types = module.type_section().map(|ts| ts.types()).unwrap_or(&[]);
		let import_entries = module
			.import_section()
			.map(|is| is.entries())
			.unwrap_or(&[]);

		let mut imported_mem_type = None;

		for import in import_entries {
			let type_idx = match import.external() {
				&External::Table(_) => return Err("Cannot import tables"),
				&External::Global(_) => return Err("Cannot import globals"),
				&External::Function(ref type_idx) => {
					if import.module() != IMPORT_MODULE_FN {
						return Err("Invalid module for imported function");
					}
					type_idx
				},
				&External::Memory(ref memory_type) => {
					if import.module() != IMPORT_MODULE_MEMORY {
						return Err("Invalid module for imported memory");
					}
					if import.field() != "memory" {
						return Err("Memory import must have the field name 'memory'")
					}
					if imported_mem_type.is_some() {
						return Err("Multiple memory imports defined")
					}
					imported_mem_type = Some(memory_type);
					continue;
				}
			};

			let Type::Function(ref func_ty) = types
				.get(*type_idx as usize)
				.ok_or_else(|| "validation: import entry points to a non-existent type")?;

			// We disallow importing `seal_println` unless debug features are enabled,
			// which should only be allowed on a dev chain
			if !self.schedule.enable_println && import.field().as_bytes() == b"seal_println" {
				return Err("module imports `seal_println` but debug features disabled");
			}

			if import_fn_banlist.iter().any(|f| import.field().as_bytes() == *f)
				|| !C::can_satisfy(import.field().as_bytes(), func_ty)
			{
				return Err("module imports a non-existent function");
			}
		}
		Ok(imported_mem_type)
	}

	fn into_wasm_code(self) -> Result<Vec<u8>, &'static str> {
		elements::serialize(self.module)
			.map_err(|_| "error serializing instrumented module")
	}
}

fn get_memory_limits<T: Config>(module: Option<&MemoryType>, schedule: &Schedule<T>)
	-> Result<(u32, u32), &'static str>
{
	if let Some(memory_type) = module {
		// Inspect the module to extract the initial and maximum page count.
		let limits = memory_type.limits();
		match (limits.initial(), limits.maximum()) {
			(initial, Some(maximum)) if initial > maximum => {
				return Err(
					"Requested initial number of pages should not exceed the requested maximum",
				);
			}
			(_, Some(maximum)) if maximum > schedule.limits.memory_pages => {
				return Err("Maximum number of pages should not exceed the configured maximum.");
			}
			(initial, Some(maximum)) => Ok((initial, maximum)),
			(_, None) => {
				// Maximum number of pages should be always declared.
				// This isn't a hard requirement and can be treated as a maximum set
				// to configured maximum.
				return Err("Maximum number of pages should be always declared.");
			}
		}
	} else {
		// If none memory imported then just crate an empty placeholder.
		// Any access to it will lead to out of bounds trap.
		Ok((0, 0))
	}
}

/// Loads the given module given in `original_code`, performs some checks on it and
/// does some preprocessing.
///
/// The checks are:
///
/// - provided code is a valid wasm module.
/// - the module doesn't define an internal memory instance,
/// - imported memory (if any) doesn't reserve more memory than permitted by the `schedule`,
/// - all imported functions from the external environment matches defined by `env` module,
///
/// The preprocessing includes injecting code for gas metering and metering the height of stack.
pub fn prepare_contract<C: ImportSatisfyCheck, T: Config>(
	original_code: &[u8],
	schedule: &Schedule<T>,
) -> Result<PrefabWasmModule, &'static str> {
	let mut contract_module = ContractModule::new(original_code, schedule)?;
	contract_module.scan_exports()?;
	contract_module.ensure_no_internal_memory()?;
	contract_module.ensure_table_size_limit(schedule.limits.table_size)?;
	contract_module.ensure_global_variable_limit(schedule.limits.globals)?;
	contract_module.ensure_no_floating_types()?;
	contract_module.ensure_parameter_limit(schedule.limits.parameters)?;
	contract_module.ensure_br_table_size_limit(schedule.limits.br_table_size)?;

	// We disallow importing `gas` function here since it is treated as implementation detail.
	let disallowed_imports = [b"gas".as_ref()];
	let memory_limits = get_memory_limits(
		contract_module.scan_imports::<C>(&disallowed_imports)?,
		schedule
	)?;

	contract_module = contract_module
		.inject_gas_metering()?
		.inject_stack_height_metering()?;

	Ok(PrefabWasmModule {
		schedule_version: schedule.version,
		initial: memory_limits.0,
		maximum: memory_limits.1,
		_reserved: None,
		code: contract_module.into_wasm_code()?,
	})
}

/// Alternate (possibly unsafe) preparation functions used only for benchmarking.
///
/// For benchmarking we need to construct special contracts that might not pass our
/// sanity checks or need to skip instrumentation for correct results. We hide functions
/// allowing this behind a feature that is only set during benchmarking to prevent usage
/// in production code.
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking {
	use super::{
		Config, ContractModule, PrefabWasmModule, ImportSatisfyCheck, Schedule, get_memory_limits
	};
	use parity_wasm::elements::FunctionType;

	impl ImportSatisfyCheck for () {
		fn can_satisfy(_name: &[u8], _func_type: &FunctionType) -> bool {
			true
		}
	}

	/// Prepare function that neither checks nor instruments the passed in code.
	pub fn prepare_contract<T: Config>(original_code: &[u8], schedule: &Schedule<T>)
		-> Result<PrefabWasmModule, &'static str>
	{
		let contract_module = ContractModule::new(original_code, schedule)?;
		let memory_limits = get_memory_limits(contract_module.scan_imports::<()>(&[])?, schedule)?;
		Ok(PrefabWasmModule {
			schedule_version: schedule.version,
			initial: memory_limits.0,
			maximum: memory_limits.1,
			_reserved: None,
			code: contract_module.into_wasm_code()?,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{exec::Ext, Limits};
	use std::fmt;
	use assert_matches::assert_matches;

	impl fmt::Debug for PrefabWasmModule {
		fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
			write!(f, "PreparedContract {{ .. }}")
		}
	}

	// Define test environment for tests. We need ImportSatisfyCheck
	// implementation from it. So actual implementations doesn't matter.
	define_env!(TestEnv, <E: Ext>,
		panic(_ctx) => { unreachable!(); },

		// gas is an implementation defined function and a contract can't import it.
		gas(_ctx, _amount: u32) => { unreachable!(); },

		nop(_ctx, _unused: u64) => { unreachable!(); },

		seal_println(_ctx, _ptr: u32, _len: u32) => { unreachable!(); },
	);

	macro_rules! prepare_test {
		($name:ident, $wat:expr, $($expected:tt)*) => {
			#[test]
			fn $name() {
				let wasm = wat::parse_str($wat).unwrap();
				let schedule = Schedule {
					limits: Limits {
						globals: 3,
						parameters: 3,
						memory_pages: 16,
						table_size: 3,
						br_table_size: 3,
						.. Default::default()
					},
					.. Default::default()
				};
				let r = prepare_contract::<TestEnv, crate::tests::Test>(wasm.as_ref(), &schedule);
				assert_matches!(r, $($expected)*);
			}
		};
	}

	prepare_test!(no_floats,
		r#"
		(module
			(func (export "call")
				(drop
					(f32.add
						(f32.const 0)
						(f32.const 1)
					)
				)
			)
			(func (export "deploy"))
		)"#,
		Err("gas instrumentation failed")
	);

	mod functions {
		use super::*;

		prepare_test!(param_number_valid,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
				(func (param i32 i32 i32))
			)
			"#,
			Ok(_)
		);

		prepare_test!(param_number_invalid,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
				(func (param i32 i32 i32 i32))
				(func (param i32))
			)
			"#,
			Err("Use of a function type with too many parameters.")
		);
	}

	mod globals {
		use super::*;

		prepare_test!(global_number_valid,
			r#"
			(module
				(global i64 (i64.const 0))
				(global i64 (i64.const 0))
				(global i64 (i64.const 0))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(global_number_too_high,
			r#"
			(module
				(global i64 (i64.const 0))
				(global i64 (i64.const 0))
				(global i64 (i64.const 0))
				(global i64 (i64.const 0))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module declares too many globals")
		);
	}

	mod memories {
		use super::*;

		prepare_test!(memory_with_one_page,
			r#"
			(module
				(import "env" "memory" (memory 1 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(internal_memory_declaration,
			r#"
			(module
				(memory 1 1)

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module declares internal memory")
		);

		prepare_test!(no_memory_import,
			r#"
			(module
				;; no memory imported

				(func (export "call"))
				(func (export "deploy"))
			)"#,
			Ok(_)
		);

		prepare_test!(initial_exceeds_maximum,
			r#"
			(module
				(import "env" "memory" (memory 16 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Module is not valid")
		);

		prepare_test!(no_maximum,
			r#"
			(module
				(import "env" "memory" (memory 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Maximum number of pages should be always declared.")
		);

		prepare_test!(requested_maximum_valid,
			r#"
			(module
				(import "env" "memory" (memory 1 16))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(requested_maximum_exceeds_configured_maximum,
			r#"
			(module
				(import "env" "memory" (memory 1 17))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Maximum number of pages should not exceed the configured maximum.")
		);

		prepare_test!(field_name_not_memory,
			r#"
			(module
				(import "env" "forgetit" (memory 1 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Memory import must have the field name 'memory'")
		);

		prepare_test!(multiple_memory_imports,
			r#"
			(module
				(import "env" "memory" (memory 1 1))
				(import "env" "memory" (memory 1 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Module is not valid")
		);

		prepare_test!(table_import,
			r#"
			(module
				(import "seal0" "table" (table 1 anyfunc))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Cannot import tables")
		);

		prepare_test!(global_import,
			r#"
			(module
				(global $g (import "seal0" "global") i32)
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Cannot import globals")
		);
	}

	mod tables {
		use super::*;

		prepare_test!(no_tables,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(table_valid_size,
			r#"
			(module
				(table 3 funcref)

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(table_too_big,
			r#"
			(module
				(table 4 funcref)

				(func (export "call"))
				(func (export "deploy"))
			)"#,
			Err("table exceeds maximum size allowed")
		);

		prepare_test!(br_table_valid_size,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
				(func
					i32.const 0
					br_table 0 0 0 0
				)
			)
			"#,
			Ok(_)
		);

		prepare_test!(br_table_too_big,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
				(func
					i32.const 0
					br_table 0 0 0 0 0
				)
			)"#,
			Err("BrTable's immediate value is too big.")
		);
	}

	mod imports {
		use super::*;

		prepare_test!(can_import_legit_function,
			r#"
			(module
				(import "seal0" "nop" (func (param i64)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		// even though gas is defined the contract can't import it since
		// it is an implementation defined.
		prepare_test!(can_not_import_gas_function,
			r#"
			(module
				(import "seal0" "gas" (func (param i32)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module imports a non-existent function")
		);

		// memory is in "env" and not in "seal0"
		prepare_test!(memory_not_in_seal0,
			r#"
			(module
				(import "seal0" "memory" (memory 1 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Invalid module for imported memory")
		);

		// memory is in "env" and not in some arbitrary module
		prepare_test!(memory_not_in_arbitrary_module,
			r#"
			(module
				(import "any_module" "memory" (memory 1 1))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Invalid module for imported memory")
		);

		// functions are in "env" and not in "seal0"
		prepare_test!(function_not_in_env,
			r#"
			(module
				(import "env" "nop" (func (param i64)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Invalid module for imported function")
		);

		// functions are in "seal0" and not in in some arbitrary module
		prepare_test!(function_not_arbitrary_module,
			r#"
			(module
				(import "any_module" "nop" (func (param i64)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("Invalid module for imported function")
		);

		// wrong signature
		prepare_test!(wrong_signature,
			r#"
			(module
				(import "seal0" "gas" (func (param i64)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module imports a non-existent function")
		);

		prepare_test!(unknown_func_name,
			r#"
			(module
				(import "seal0" "unknown_func" (func))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module imports a non-existent function")
		);

		prepare_test!(seal_println_debug_disabled,
			r#"
			(module
				(import "seal0" "seal_println" (func $seal_println (param i32 i32)))

				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("module imports `seal_println` but debug features disabled")
		);

		#[test]
		fn seal_println_debug_enabled() {
			let wasm = wat::parse_str(
				r#"
				(module
					(import "seal0" "seal_println" (func $seal_println (param i32 i32)))

					(func (export "call"))
					(func (export "deploy"))
				)
				"#
			).unwrap();
			let mut schedule = Schedule::default();
			schedule.enable_println = true;
			let r = prepare_contract::<TestEnv, crate::tests::Test>(wasm.as_ref(), &schedule);
			assert_matches!(r, Ok(_));
		}
	}

	mod entrypoints {
		use super::*;

		prepare_test!(it_works,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Ok(_)
		);

		prepare_test!(omit_deploy,
			r#"
			(module
				(func (export "call"))
			)
			"#,
			Err("deploy function isn't exported")
		);

		prepare_test!(omit_call,
			r#"
			(module
				(func (export "deploy"))
			)
			"#,
			Err("call function isn't exported")
		);

		// Try to use imported function as an entry point.
		prepare_test!(try_sneak_export_as_entrypoint,
			r#"
			(module
				(import "seal0" "panic" (func))

				(func (export "deploy"))

				(export "call" (func 0))
			)
			"#,
			Err("entry point points to an imported function")
		);

		// Try to use imported function as an entry point.
		prepare_test!(try_sneak_export_as_global,
			r#"
			(module
				(func (export "deploy"))
				(global (export "call") i32 (i32.const 0))
			)
			"#,
			Err("expected a function")
		);

		prepare_test!(wrong_signature,
			r#"
			(module
				(func (export "deploy"))
				(func (export "call") (param i32))
			)
			"#,
			Err("entry point has wrong signature")
		);

		prepare_test!(unknown_exports,
			r#"
			(module
				(func (export "call"))
				(func (export "deploy"))
				(func (export "whatevs"))
			)
			"#,
			Err("unknown export: expecting only deploy and call functions")
		);

		prepare_test!(global_float,
			r#"
			(module
				(global $x f32 (f32.const 0))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("use of floating point type in globals is forbidden")
		);

		prepare_test!(local_float,
			r#"
			(module
				(func $foo (local f32))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("use of floating point type in locals is forbidden")
		);

		prepare_test!(param_float,
			r#"
			(module
				(func $foo (param f32))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("use of floating point type in function types is forbidden")
		);

		prepare_test!(result_float,
			r#"
			(module
				(func $foo (result f32) (f32.const 0))
				(func (export "call"))
				(func (export "deploy"))
			)
			"#,
			Err("use of floating point type in function types is forbidden")
		);
	}
}
