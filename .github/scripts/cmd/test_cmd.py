import unittest
from unittest.mock import patch, mock_open, MagicMock, call
import json
import sys
import os
import argparse

# Mock data for runtimes-matrix.json
mock_runtimes_matrix = [
    {"name": "dev", "package": "kitchensink-runtime", "path": "substrate/frame", "header": "substrate/HEADER-APACHE2",  "template": "substrate/.maintain/frame-weight-template.hbs", "bench_features": "runtime-benchmarks,riscv"},
    {"name": "westend", "package": "westend-runtime", "path": "polkadot/runtime/westend", "header": "polkadot/file_header.txt", "template": "polkadot/xcm/pallet-xcm-benchmarks/template.hbs", "bench_features": "runtime-benchmarks"},
    {"name": "rococo", "package": "rococo-runtime", "path": "polkadot/runtime/rococo", "header": "polkadot/file_header.txt", "template": "polkadot/xcm/pallet-xcm-benchmarks/template.hbs", "bench_features": "runtime-benchmarks"},
    {"name": "asset-hub-westend", "package": "asset-hub-westend-runtime", "path": "cumulus/parachains/runtimes/assets/asset-hub-westend", "header": "cumulus/file_header.txt", "template": "cumulus/templates/xcm-bench-template.hbs", "bench_features": "runtime-benchmarks"},
]

def get_mock_bench_output(runtime, pallets, output_path, header, template = None):
    return f"frame-omni-bencher v1 benchmark pallet --extrinsic=* " \
           f"--runtime=target/release/wbuild/{runtime}-runtime/{runtime.replace('-', '_')}_runtime.wasm " \
           f"--pallet={pallets} --header={header} " \
           f"--output={output_path} " \
           f"--wasm-execution=compiled " \
           f"--steps=50 --repeat=20 --heap-pages=4096 " \
           f"{f'--template={template} ' if template else ''}" \
           f"--no-storage-info --no-min-squares --no-median-slopes"

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
            continue_on_fail=False,
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
                
                call(get_mock_bench_output('kitchensink', 'pallet_balances', './substrate/frame/balances/src/weights.rs', os.path.abspath('substrate/HEADER-APACHE2'), "substrate/.maintain/frame-weight-template.hbs")),
                call(get_mock_bench_output('westend', 'pallet_balances', './polkadot/runtime/westend/src/weights', os.path.abspath('polkadot/file_header.txt'))),
                # skips rococo benchmark
                call(get_mock_bench_output('asset-hub-westend', 'pallet_balances', './cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights', os.path.abspath('cumulus/file_header.txt'))),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_normal_execution(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend'],
            pallet=['pallet_balances', 'pallet_staking'],
            continue_on_fail=False,
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
                call(get_mock_bench_output('westend', 'pallet_balances', './polkadot/runtime/westend/src/weights', header_path)),
                call(get_mock_bench_output('westend', 'pallet_staking', './polkadot/runtime/westend/src/weights', header_path)),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)


    def test_bench_command_normal_execution_xcm(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend'],
            pallet=['pallet_xcm_benchmarks::generic'],
            continue_on_fail=False,
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
                    'westend', 
                    'pallet_xcm_benchmarks::generic', 
                    './polkadot/runtime/westend/src/weights/xcm', 
                    header_path, 
                    "polkadot/xcm/pallet-xcm-benchmarks/template.hbs"
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_two_runtimes_two_pallets(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['westend', 'rococo'],
            pallet=['pallet_balances', 'pallet_staking'],
            continue_on_fail=False,
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
                call(get_mock_bench_output('westend', 'pallet_staking', './polkadot/runtime/westend/src/weights', header_path)),
                call(get_mock_bench_output('westend', 'pallet_balances', './polkadot/runtime/westend/src/weights', header_path)),
                # Rococo runtime calls
                call(get_mock_bench_output('rococo', 'pallet_staking', './polkadot/runtime/rococo/src/weights', header_path)),
                call(get_mock_bench_output('rococo', 'pallet_balances', './polkadot/runtime/rococo/src/weights', header_path)),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_dev_runtime(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['dev'],
            pallet=['pallet_balances'],
            continue_on_fail=False,
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
                    'kitchensink', 
                    'pallet_balances', 
                    manifest_dir + "/src/weights.rs", 
                    header_path, 
                    "substrate/.maintain/frame-weight-template.hbs"
                )),
            ]
            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_cumulus_runtime(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['asset-hub-westend'],
            pallet=['pallet_assets'],
            continue_on_fail=False,
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
                    'asset-hub-westend', 
                    'pallet_assets', 
                    './cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights', 
                    header_path
                )),
            ]

            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    def test_bench_command_one_cumulus_runtime_xcm(self):
        self.mock_parse_args.return_value = (argparse.Namespace(
            command='bench',
            runtime=['asset-hub-westend'],
            pallet=['pallet_xcm_benchmarks::generic', 'pallet_assets'],
            continue_on_fail=False,
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
                    'asset-hub-westend', 
                    'pallet_xcm_benchmarks::generic', 
                    './cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights/xcm', 
                    header_path, 
                    "cumulus/templates/xcm-bench-template.hbs"
                )),
                call(get_mock_bench_output(
                    'asset-hub-westend', 
                    'pallet_assets', 
                    './cumulus/parachains/runtimes/assets/asset-hub-westend/src/weights', 
                    header_path
                )),
            ]

            self.mock_system.assert_has_calls(expected_calls, any_order=True)

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='fmt', continue_on_fail=False), []))
    @patch('os.system', return_value=0)
    def test_fmt_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            mock_system.assert_any_call('cargo +nightly fmt')
            mock_system.assert_any_call('taplo format --config .config/taplo.toml')

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='update-ui', continue_on_fail=False), []))
    @patch('os.system', return_value=0)
    def test_update_ui_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            mock_system.assert_called_with('sh ./scripts/update-ui-tests.sh')

    @patch('argparse.ArgumentParser.parse_known_args', return_value=(argparse.Namespace(command='prdoc', continue_on_fail=False), []))
    @patch('os.system', return_value=0)
    def test_prdoc_command(self, mock_system, mock_parse_args):
        with patch('sys.exit') as mock_exit:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()
            self.mock_generate_prdoc_main.assert_called_with(mock_parse_args.return_value[0])

if __name__ == '__main__':
    unittest.main()