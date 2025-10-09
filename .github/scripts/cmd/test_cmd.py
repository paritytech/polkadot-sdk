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
        "bench_features": "runtime-benchmarks",
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
           f"--runtime=target/production/wbuild/{runtime}-runtime/{runtime.replace('-', '_')}_runtime.wasm " \
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
            command='bench-omni',
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
                call("forklift cargo build -q -p kitchensink-runtime --profile production --features=runtime-benchmarks"),
                call("forklift cargo build -q -p westend-runtime --profile production --features=runtime-benchmarks"),
                call("forklift cargo build -q -p rococo-runtime --profile production --features=runtime-benchmarks"),
                call("forklift cargo build -q -p asset-hub-westend-runtime --profile production --features=runtime-benchmarks"),

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
            command='bench-omni',
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
                call("forklift cargo build -q -p westend-runtime --profile production --features=runtime-benchmarks"),

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
            command='bench-omni',
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
                call("forklift cargo build -q -p westend-runtime --profile production --features=runtime-benchmarks"),

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
            command='bench-omni',
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
                call("forklift cargo build -q -p westend-runtime --profile production --features=runtime-benchmarks"),
                call("forklift cargo build -q -p rococo-runtime --profile production --features=runtime-benchmarks"),
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
            command='bench-omni',
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
                call("forklift cargo build -q -p kitchensink-runtime --profile production --features=runtime-benchmarks"),
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
            command='bench-omni',
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
                call("forklift cargo build -q -p asset-hub-westend-runtime --profile production --features=runtime-benchmarks"),
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
            command='bench-omni',
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
                call("forklift cargo build -q -p asset-hub-westend-runtime --profile production --features=runtime-benchmarks"),
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

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_valid_labels(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command with valid labels"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required', 'D2-substantial']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME', 'R0-no-crate-publish-required']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check that JSON output was printed
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)
            self.assertIn('T1-FRAME', str(json_call))
            self.assertIn('R0-no-crate-publish-required', str(json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_auto_correction(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command with auto-correctable typos"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required', 'D2-substantial']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAM', 'R0-no-crate-publish']  # Typos that should be auto-corrected
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check for auto-correction messages
            correction_messages = [str(call) for call in mock_print.call_args_list if 'Auto-corrected' in str(call)]
            self.assertTrue(len(correction_messages) > 0)

            # Check that JSON output contains corrected labels
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)
            self.assertIn('T1-FRAME', str(json_call))
            self.assertIn('R0-no-crate-publish-required', str(json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_prefix_correction(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command with prefix matching"""
        mock_get_labels.return_value = ['T1-FRAME', 'T2-pallets', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-something']  # Should match T1-FRAME as the only T1- label
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check that JSON output contains corrected label
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)
            self.assertIn('T1-FRAME', str(json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_invalid_labels(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command with invalid labels that cannot be corrected"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required', 'D2-substantial']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['INVALID-LABEL', 'ANOTHER-BAD-LABEL']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_called_with(1)  # Should exit with error code

            # Check for error JSON output
            error_json_call = None
            for call in mock_print.call_args_list:
                if 'ERROR_JSON:' in str(call):
                    error_json_call = call
                    break

            self.assertIsNotNone(error_json_call)
            self.assertIn('validation_failed', str(error_json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_mixed_valid_invalid(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command with mix of valid and invalid labels"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required', 'D2-substantial']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME', 'INVALID-LABEL', 'D2-substantial']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_called_with(1)  # Should exit with error code due to invalid label

            # Check for error JSON output
            error_json_call = None
            for call in mock_print.call_args_list:
                if 'ERROR_JSON:' in str(call):
                    error_json_call = call
                    break

            self.assertIsNotNone(error_json_call)

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_fetch_failure(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command when label fetching fails"""
        mock_get_labels.side_effect = RuntimeError("Failed to fetch labels from repository. Please check your connection and try again.")
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_called_with(1)  # Should exit with error code

            # Check for error JSON output
            error_json_call = None
            for call in mock_print.call_args_list:
                if 'ERROR_JSON:' in str(call):
                    error_json_call = call
                    break

            self.assertIsNotNone(error_json_call)
            self.assertIn('Failed to fetch labels from repository', str(error_json_call))

    def test_auto_correct_labels_function(self):
        """Test the auto_correct_labels function directly"""
        import cmd

        valid_labels = ['T1-FRAME', 'R0-no-crate-publish-required', 'D2-substantial', 'I2-bug']

        # Test high similarity auto-correction
        corrections, suggestions = cmd.auto_correct_labels(['T1-FRAM'], valid_labels)
        self.assertEqual(len(corrections), 1)
        self.assertEqual(corrections[0][0], 'T1-FRAM')
        self.assertEqual(corrections[0][1], 'T1-FRAME')

        # Test low similarity suggestions
        corrections, suggestions = cmd.auto_correct_labels(['TOTALLY-WRONG'], valid_labels)
        self.assertEqual(len(corrections), 0)
        self.assertEqual(len(suggestions), 1)

    def test_find_closest_labels_function(self):
        """Test the find_closest_labels function directly"""
        import cmd

        valid_labels = ['T1-FRAME', 'T2-pallets', 'R0-no-crate-publish-required']

        # Test finding close matches
        matches = cmd.find_closest_labels('T1-FRAM', valid_labels)
        self.assertIn('T1-FRAME', matches)

        # Test no close matches
        matches = cmd.find_closest_labels('COMPLETELY-DIFFERENT', valid_labels, cutoff=0.8)
        self.assertEqual(len(matches), 0)

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_merged_pr(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command on merged PR should fail"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = False  # PR is merged/closed
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_called_with(1)

            # Check for error JSON output
            error_json_call = None
            for call in mock_print.call_args_list:
                if 'ERROR_JSON:' in str(call):
                    error_json_call = call
                    break

            self.assertIsNotNone(error_json_call)
            self.assertIn('Cannot modify labels on merged PRs', str(error_json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_open_pr(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command on open PR should succeed"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check that JSON output was printed
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'false', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_unauthorized_user(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command by unauthorized user should fail"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_called_with(1)

            # Check for error JSON output
            error_json_call = None
            for call in mock_print.call_args_list:
                if 'ERROR_JSON:' in str(call):
                    error_json_call = call
                    break

            self.assertIsNotNone(error_json_call)
            self.assertIn('Only the PR author or organization members can modify labels', str(error_json_call))

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'false', 'IS_PR_AUTHOR': 'true', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_pr_author(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command by PR author should succeed"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check that JSON output was printed
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)

    @patch.dict('os.environ', {'PR_NUM': '123', 'IS_ORG_MEMBER': 'true', 'IS_PR_AUTHOR': 'false', 'GITHUB_TOKEN': 'fake_token'})
    @patch('cmd.get_allowed_labels')
    @patch('cmd.check_pr_status')
    @patch('argparse.ArgumentParser.parse_known_args')
    def test_label_command_org_member(self, mock_parse_args, mock_check_pr_status, mock_get_labels):
        """Test label command by org member should succeed"""
        mock_get_labels.return_value = ['T1-FRAME', 'R0-no-crate-publish-required']
        mock_check_pr_status.return_value = True  # PR is open
        mock_parse_args.return_value = (argparse.Namespace(
            command='label',
            labels=['T1-FRAME']
        ), [])

        with patch('sys.exit') as mock_exit, patch('builtins.print') as mock_print:
            import cmd
            cmd.main()
            mock_exit.assert_not_called()

            # Check that JSON output was printed
            json_call = None
            for call in mock_print.call_args_list:
                if 'LABELS_JSON:' in str(call):
                    json_call = call
                    break

            self.assertIsNotNone(json_call)

if __name__ == '__main__':
    unittest.main()
