import unittest
from unittest.mock import patch, mock_open, MagicMock, call
import json
import sys
import os
import argparse

# Mock data for runtimes-matrix.json
mock_runtimes_matrix = [
    {
        "name": "dev",
        "package": "kitchensink-runtime",
        "path": "substrate/frame",
        "header": "substrate/HEADER-APACHE2",
        "template": "substrate/.maintain/frame-weight-template.hbs",
        "bench_features": "runtime-benchmarks,riscv",
        "bench_flags": "--flag1 --flag2"
    },
    {
        "name": "westend",
        "package": "westend-runtime",
        "path": "polkadot/runtime/westend",
        "header": "polkadot/file_header.txt",
        "template": "polkadot/xcm/pallet-xcm-benchmarks/template.hbs",
        "bench_features": "runtime-benchmarks",
        "bench_flags": "--flag3 --flag4"
    },
    {
        "name": "rococo",
        "package": "rococo-runtime",
        "path": "polkadot/runtime/rococo",
        "header": "polkadot/file_header.txt",
        "template": "polkadot/xcm/pallet-xcm-benchmarks/template.hbs",
        "bench_features": "runtime-benchmarks",
        "bench_flags": ""
    },
    {
        "name": "asset-hub-westend",
        "package": "asset-hub-westend-runtime",
        "path": "cumulus/parachains/runtimes/assets/asset-hub-westend",
        "header": "cumulus/file_header.txt",
        "template": "cumulus/templates/xcm-bench-template.hbs",
        "bench_features": "runtime-benchmarks",
        "bench_flags": "--flag7 --flag8"
    }
]

def get_mock_bench_output(runtime, pallets, output_path, header, bench_flags, template = None):
    return f"frame-omni-bencher v1 benchmark pallet --extrinsic=* " \
           f"--runtime=target/release/wbuild/{runtime}-runtime/{runtime.replace('-', '_')}_runtime.wasm " \
           f"--pallet={pallets} --header={header} " \
           f"--output={output_path} " \
           f"--wasm-execution=compiled " \
           f"--steps=50 --repeat=20 --heap-pages=4096 " \
           f"{f'--template={template} ' if template else ''}" \
           f"--no-storage-info --no-min-squares --no-median-slopes " \
           f"{bench_flags}"

class TestCmd(unittest.TestCase):

    def setUp(self):
        self.patcher1 = patch('builtins.open', new_callable=mock_open, read_data=json.dumps(mock_runtimes_matrix))
        self.patcher2 = patch('json.load', return_value=mock_runtimes_matrix)
        self.patcher3 = patch('argparse.ArgumentParser.parse_known_args')
        self.patcher4 = patch('os.system', return_value=0)
        self.patcher5 = patch('os.popen')
        self.patcher6 = patch('importlib.util.spec_from_file_location', return_value=MagicMock())
        self.patcher7 = patch('importlib.util.module_from_spec', return_value=MagicMock())
        self.patcher8 = patch('cmd.generate_prdoc.main', return_value=0)
        
        self.mock_open = self.patcher1.start()
        self.mock_json_load = self.patcher2.start()
        self.mock_parse_args = self.patcher3.start()
        self.mock_system = self.patcher4.start()
        self.mock_popen = self.patcher5.start()
        self.mock_spec_from_file_location = self.patcher6.start()
        self.mock_module_from_spec = self.patcher7.start()
        self.mock_generate_prdoc_main = self.patcher8.start()

        # Ensure that cmd.py uses the mock_runtimes_matrix
        import cmd
        cmd.runtimesMatrix = mock_runtimes_matrix

    def tearDown(self):
        self.patcher1.stop()
        self.patcher2.stop()
        self.patcher3.stop()
        self.patcher4.stop()
        self.patcher5.stop()
        self.patcher6.stop()
        self.patcher7.stop()
        self.patcher8.stop()

    def test_bench_command_normal_execution_all_runtimes(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=list(map(lambda x: x['name'], mock_runtimes_matrix)),
            pallet=['pallet_balances'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        
        self.mock_popen.return_value.read.side_effect = [
            "pallet_balances\npallet_staking\npallet_something\n",  # Output for dev runtime
            "pallet_balances\npallet_staking\npallet_something\n",  # Output for westend runtime
            "pallet_staking\npallet_something\n",                   # Output for rococo runtime - no pallet here
            "pallet_balances\npallet_staking\npallet_something\n",  # Output for asset-hub-westend runtime
            "./substrate/frame/balances/Cargo.toml\n",                # Mock manifest path for dev -> pallet_balances
        ]
        
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p kitchensink-runtime --profile release --features=runtime-benchmarks,riscv"),
                call("forklift cargo build -p westend-runtime --profile release --features=runtime-benchmarks"),
                call("forklift cargo build -p rococo-runtime --profile release --features=runtime-benchmarks"),
                call("forklift cargo build -p asset-hub-westend-runtime --profile release --features=runtime-benchmarks"),
                
                call(get_mock_bench_output(
                    runtime='kitchensink',
                    pallets='pallet_balances',
                    output_path='./substrate/frame/balances/src/weights.rs',
                    header=os.path.abspath('substrate/HEADER-APACHE2'),
                    bench_flags='--flag1 --flag2',
                    template="substrate/.maintain/frame-weight-template.hbs"
                )),
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_balances',
                    output_path='./polkadot/runtime/westend/src/weights',
                    header=os.path.abspath('polkadot/file_header.txt'),
                    bench_flags='--flag3 --flag4'
                )),
                # skips rococo benchmark
                call(get_mock_bench_output(
                    runtime='asset-hub-westend',
                    pallets='pallet_balances',
                    output_path='./cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights',
                    header=os.path.abspath('cumulus/file_header.txt'),
                    bench_flags='--flag7 --flag8'
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_normal_execution(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend'],
            pallet=['pallet_balances', 'pallet_staking'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        header_path = os.path.abspath('polkadot/file_header.txt')
        self.mock_popen.return_value.read.side_effect = [
            "pallet_balances\npallet_staking\npallet_something\n",  # Output for westend runtime
        ]
        
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p westend-runtime --profile release --features=runtime-benchmarks"),
                
                # Westend runtime calls
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_balances',
                    output_path='./polkadot/runtime/westend/src/weights',
                    header=header_path,
                    bench_flags='--flag3 --flag4'
                )),
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_staking',
                    output_path='./polkadot/runtime/westend/src/weights',
                    header=header_path,
                    bench_flags='--flag3 --flag4'
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)


    def test_bench_command_normal_execution_xcm(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend'],
            pallet=['pallet_xcm_benchmarks::generic'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        header_path = os.path.abspath('polkadot/file_header.txt')
        self.mock_popen.return_value.read.side_effect = [
            "pallet_balances\npallet_staking\npallet_something\npallet_xcm_benchmarks::generic\n",  # Output for westend runtime
        ]
        
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p westend-runtime --profile release --features=runtime-benchmarks"),
                
                # Westend runtime calls
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_xcm_benchmarks::generic',
                    output_path='./polkadot/runtime/westend/src/weights/xcm',
                    header=header_path,
                    bench_flags='--flag3 --flag4',
                    template="polkadot/xcm/pallet-xcm-benchmarks/template.hbs"
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_two_runtimes_two_pallets(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend', 'rococo'],
            pallet=['pallet_balances', 'pallet_staking'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        self.mock_popen.return_value.read.side_effect = [
            "pallet_staking\npallet_balances\n",  # Output for westend runtime
            "pallet_staking\npallet_balances\n",  # Output for rococo runtime
        ]
        
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            header_path = os.path.abspath('polkadot/file_header.txt')

            expected_calls = [
                # Build calls
                call("forklift cargo build -p westend-runtime --profile release --features=runtime-benchmarks"),
                call("forklift cargo build -p rococo-runtime --profile release --features=runtime-benchmarks"),
                # Westend runtime calls
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_staking',
                    output_path='./polkadot/runtime/westend/src/weights',
                    header=header_path,
                    bench_flags='--flag3 --flag4'
                )),
                call(get_mock_bench_output(
                    runtime='westend',
                    pallets='pallet_balances',
                    output_path='./polkadot/runtime/westend/src/weights',
                    header=header_path,
                    bench_flags='--flag3 --flag4'
                )),
                # Rococo runtime calls
                call(get_mock_bench_output(
                    runtime='rococo',
                    pallets='pallet_staking',
                    output_path='./polkadot/runtime/rococo/src/weights',
                    header=header_path,
                    bench_flags=''
                )),
                call(get_mock_bench_output(
                    runtime='rococo',
                    pallets='pallet_balances',
                    output_path='./polkadot/runtime/rococo/src/weights',
                    header=header_path,
                    bench_flags=''
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_dev_runtime(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['dev'],
            pallet=['pallet_balances'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        manifest_dir = "substrate/frame/kitchensink"
        self.mock_popen.return_value.read.side_effect = [
            "pallet_balances\npallet_something",  # Output for dev runtime
            manifest_dir + "/Cargo.toml"  # Output for manifest path in dev runtime
        ]
        header_path = os.path.abspath('substrate/HEADER-APACHE2')

        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p kitchensink-runtime --profile release --features=runtime-benchmarks,riscv"),
                # Westend runtime calls
                call(get_mock_bench_output(
                    runtime='kitchensink',
                    pallets='pallet_balances',
                    output_path=manifest_dir + "/src/weights.rs",
                    header=header_path,
                    bench_flags='--flag1 --flag2',
                    template="substrate/.maintain/frame-weight-template.hbs"
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_cumulus_runtime(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['asset-hub-westend'],
            pallet=['pallet_assets'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        self.mock_popen.return_value.read.side_effect = [
            "pallet_assets\n",  # Output for asset-hub-westend runtime
        ]
        header_path = os.path.abspath('cumulus/file_header.txt')

        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p asset-hub-westend-runtime --profile release --features=runtime-benchmarks"),
                # Asset-hub-westend runtime calls
                call(get_mock_bench_output(
                    runtime='asset-hub-westend',
                    pallets='pallet_assets',
                    output_path='./cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights',
                    header=header_path,
                    bench_flags='--flag7 --flag8'
                )),
            ]

            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_cumulus_runtime_xcm(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['asset-hub-westend'],
            pallet=['pallet_xcm_benchmarks::generic', 'pallet_assets'],
            fail_fast=True,
            quiet=False,
            clean=False,
            image=None
        ), [])
        self.mock_popen.return_value.read.side_effect = [
            "pallet_assets\npallet_xcm_benchmarks::generic\n",  # Output for asset-hub-westend runtime
        ]
        header_path = os.path.abspath('cumulus/file_header.txt')

        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            expected_calls = [
                # Build calls
                call("forklift cargo build -p asset-hub-westend-runtime --profile release --features=runtime-benchmarks"),
                # Asset-hub-westend runtime calls
                call(get_mock_bench_output(
                    runtime='asset-hub-westend',
                    pallets='pallet_xcm_benchmarks::generic',
                    output_path='./cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights/xcm',
                    header=header_path,
                    bench_flags='--flag7 --flag8',
                    template="cumulus/templates/xcm-bench-template.hbs"
                )),
                call(get_mock_bench_output(
                    runtime='asset-hub-westend',
                    pallets='pallet_assets',
                    output_path='./cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights',
                    header=header_path,
                    bench_flags='--flag7 --flag8'
                )),
            ]

            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='fmt'), []))
    @patch('os.system', return_value=0)
    def test_fmt_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            mock_system.assert_any_call('cargo +nightly fmt')
            mock_system.assert_any_call('taplo format --config .config/taplo.toml')

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='update-ui'), []))
    @patch('os.system', return_value=0)
    def test_update_ui_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            mock_system.assert_called_with('sh ./scripts/update-ui-tests.sh')

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='prdoc'), []))
    @patch('os.system', return_value=0)
    def test_prdoc_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            self.mock_generate_prdoc_main.assert_called_with(mock_parse_args.return_value[0])

if __name__ == '__main__':
    unittest.main()