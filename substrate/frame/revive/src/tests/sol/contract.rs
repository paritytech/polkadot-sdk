// See the License for the specific language governing permissions and
// limitations under the License.

//! The pallet-revive shared VM integration test suite.

use crate::{
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};
use alloy_core::{primitives::U256, sol, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Caller, FixtureType};
use pretty_assertions::assert_eq;

/// Tests that the `CALL` opcode works as expected by having one contract call another.
#[test]
fn call_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let _ = sp_tracing::try_init_simple();

		let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
		let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Instantiate the callee contract, which can echo a value.
			let Contract { addr: callee_addr, .. } =
				builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

			log::info!("Callee  addr: {:?}", callee_addr);

			// Instantiate the caller contract.
			let Contract { addr: caller_addr, .. } =
				builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

			log::info!("Caller  addr: {:?}", caller_addr);

			let magic_number = U256::from(42);
			log::info!("Calling callee from caller");
			let result = builder::bare_call(caller_addr)
				.data(
					Caller::CallerCalls::do_call(Caller::do_callCall {
						callee: callee_addr.0.into(),
						value: magic_number,
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();

			let returned_value = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
			assert_eq!(returned_value, magic_number);
		});
	}
}
