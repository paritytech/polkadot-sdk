#!/usr/bin/env python3

import os
import sys
import json
import argparse
import _help
import importlib.util

_HelpAction = _help._HelpAction

f = open('.github/workflows/runtimes-matrix.json', 'r')
runtimesMatrix = json.load(f)

runtimeNames = list(map(lambda x: x['name'], runtimesMatrix))

common_args = {
    '--quiet': {"action": "store_true", "help": "Won't print start/end/failed messages in PR"},
    '--clean': {"action": "store_true", "help": "Clean up the previous bot's & author's comments in PR"},
    '--image': {"help": "Override docker image '--image docker.io/paritytech/ci-unified:latest'"},
}

def print_and_log(message, output_file='/tmp/cmd/command_output.log'):
    print(message)
    with open(output_file, 'a') as f:
        f.write(message + '\n')

def setup_logging():
    if not os.path.exists('/tmp/cmd'):
        os.makedirs('/tmp/cmd')
    open('/tmp/cmd/command_output.log', 'w')

parser = argparse.ArgumentParser(prog="/cmd ", description='A command runner for polkadot-sdk repo', add_help=False)
parser.add_argument('--help', action=_HelpAction, help='help for help if you need some help')  # help for help
for arg, config in common_args.items():
    parser.add_argument(arg, **config)

subparsers = parser.add_subparsers(help='a command to run', dest='command')

setup_logging()

"""
BENCH 
"""

bench_example = '''**Examples**:
 Runs all benchmarks 
 %(prog)s

 Runs benchmarks for pallet_balances and pallet_multisig for all runtimes which have these pallets. **--quiet** makes it to output nothing to PR but reactions
 %(prog)s --pallet pallet_balances pallet_xcm_benchmarks::generic --quiet
 
 Runs bench for all pallets for westend runtime and fails fast on first failed benchmark
 %(prog)s --runtime westend --fail-fast
 
 Does not output anything and cleans up the previous bot's & author command triggering comments in PR 
 %(prog)s --runtime westend rococo --pallet pallet_balances pallet_multisig --quiet --clean
'''

parser_bench = subparsers.add_parser('bench', help='Runs benchmarks', epilog=bench_example, formatter_class=argparse.RawDescriptionHelpFormatter)

for arg, config in common_args.items():
    parser_bench.add_argument(arg, **config)

parser_bench.add_argument('--runtime', help='Runtime(s) space separated', choices=runtimeNames, nargs='*', default=runtimeNames)
parser_bench.add_argument('--pallet', help='Pallet(s) space separated', nargs='*', default=[])
parser_bench.add_argument('--fail-fast', help='Fail fast on first failed benchmark', action='store_true')

"""
FMT 
"""
parser_fmt = subparsers.add_parser('fmt', help='Formats code (cargo +nightly-VERSION fmt) and configs (taplo format)')
for arg, config in common_args.items():
    parser_fmt.add_argument(arg, **config)

"""
Update UI 
"""
parser_ui = subparsers.add_parser('update-ui', help='Updates UI tests')
for arg, config in common_args.items():
    parser_ui.add_argument(arg, **config)

"""
PRDOC
"""
# Import generate-prdoc.py dynamically
spec = importlib.util.spec_from_file_location("generate_prdoc", ".github/scripts/generate-prdoc.py")
generate_prdoc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generate_prdoc)

parser_prdoc = subparsers.add_parser('prdoc', help='Generates PR documentation')
generate_prdoc.setup_parser(parser_prdoc, pr_required=False)

def main():
    global args, unknown, runtimesMatrix
    args, unknown = parser.parse_known_args()

    print(f'args: {args}')

    if args.command == 'bench':
        runtime_pallets_map = {}
        failed_benchmarks = {}
        successful_benchmarks = {}

        profile = "release"

        print(f'Provided runtimes: {args.runtime}')
        # convert to mapped dict
        runtimesMatrix = list(filter(lambda x: x['name'] in args.runtime, runtimesMatrix))
        runtimesMatrix = {x['name']: x for x in runtimesMatrix}
        print(f'Filtered out runtimes: {runtimesMatrix}')

        # loop over remaining runtimes to collect available pallets
        for runtime in runtimesMatrix.values():
            os.system(f"forklift cargo build -p {runtime['package']} --profile {profile} --features={runtime['bench_features']}")
            print(f'-- listing pallets for benchmark for {runtime["name"]}')
            wasm_file = f"target/{profile}/wbuild/{runtime['package']}/{runtime['package'].replace('-', '_')}.wasm"
            output = os.popen(
                f"frame-omni-bencher v1 benchmark pallet --no-csv-header --no-storage-info --no-min-squares --no-median-slopes --all --list --runtime={wasm_file} {runtime['bench_flags']}").read()
            raw_pallets = output.strip().split('\n')

            all_pallets = set()
            for pallet in raw_pallets:
                if pallet:
                    all_pallets.add(pallet.split(',')[0].strip())

            pallets = list(all_pallets)
            print(f'Pallets in {runtime["name"]}: {pallets}')
            runtime_pallets_map[runtime['name']] = pallets

        print(f'\n')

        # filter out only the specified pallets from collected runtimes/pallets
        if args.pallet:
            print(f'Pallets: {args.pallet}')
            new_pallets_map = {}
            # keep only specified pallets if they exist in the runtime
            for runtime in runtime_pallets_map:
                if set(args.pallet).issubset(set(runtime_pallets_map[runtime])):
                    new_pallets_map[runtime] = args.pallet

            runtime_pallets_map = new_pallets_map

        print(f'Filtered out runtimes & pallets: {runtime_pallets_map}\n')

        if not runtime_pallets_map:
            if args.pallet and not args.runtime:
                print(f"No pallets {args.pallet} found in any runtime")
            elif args.runtime and not args.pallet:
                print(f"{args.runtime} runtime does not have any pallets")
            elif args.runtime and args.pallet:
                print(f"No pallets {args.pallet} found in {args.runtime}")
            else:
                print('No runtimes found')
            sys.exit(1)

        for runtime in runtime_pallets_map:
            for pallet in runtime_pallets_map[runtime]:
                config = runtimesMatrix[runtime]
                header_path = os.path.abspath(config['header'])
                template = None

                print(f'-- config: {config}')
                if runtime == 'dev':
                    # to support sub-modules (https://github.com/paritytech/command-bot/issues/275)
                    search_manifest_path = f"cargo metadata --locked --format-version 1 --no-deps | jq -r '.packages[] | select(.name == \"{pallet.replace('_', '-')}\") | .manifest_path'"
                    print(f'-- running: {search_manifest_path}')
                    manifest_path = os.popen(search_manifest_path).read()
                    if not manifest_path:
                        print(f'-- pallet {pallet} not found in dev runtime')
                        if args.fail_fast:
                            print_and_log(f'Error: {pallet} not found in dev runtime')
                            sys.exit(1)
                    package_dir = os.path.dirname(manifest_path)
                    print(f'-- package_dir: {package_dir}')
                    print(f'-- manifest_path: {manifest_path}')
                    output_path = os.path.join(package_dir, "src", "weights.rs")
                    template = config['template']
                else:
                    default_path = f"./{config['path']}/src/weights"
                    xcm_path = f"./{config['path']}/src/weights/xcm"
                    output_path = default_path
                    if pallet.startswith("pallet_xcm_benchmarks"):
                        template = config['template']
                        output_path = xcm_path

                print(f'-- benchmarking {pallet} in {runtime} into {output_path}')
                cmd = f"frame-omni-bencher v1 benchmark pallet " \
                    f"--extrinsic=* " \
                    f"--runtime=target/{profile}/wbuild/{config['package']}/{config['package'].replace('-', '_')}.wasm " \
                    f"--pallet={pallet} " \
                    f"--header={header_path} " \
                    f"--output={output_path} " \
                    f"--wasm-execution=compiled " \
                    f"--steps=50 " \
                    f"--repeat=20 " \
                    f"--heap-pages=4096 " \
                    f"{f'--template={template} ' if template else ''}" \
                    f"--no-storage-info --no-min-squares --no-median-slopes " \
                    f"{config['bench_flags']}"
                print(f'-- Running: {cmd} \n')
                status = os.system(cmd)

                if status != 0 and args.fail_fast:
                    print_and_log(f'❌ Failed to benchmark {pallet} in {runtime}')
                    sys.exit(1)

                # Otherwise collect failed benchmarks and print them at the end
                # push failed pallets to failed_benchmarks
                if status != 0:
                    failed_benchmarks[f'{runtime}'] = failed_benchmarks.get(f'{runtime}', []) + [pallet]
                else:
                    successful_benchmarks[f'{runtime}'] = successful_benchmarks.get(f'{runtime}', []) + [pallet]

        if failed_benchmarks:
            print_and_log('❌ Failed benchmarks of runtimes/pallets:')
            for runtime, pallets in failed_benchmarks.items():
                print_and_log(f'-- {runtime}: {pallets}')

        if successful_benchmarks:
            print_and_log('✅ Successful benchmarks of runtimes/pallets:')
            for runtime, pallets in successful_benchmarks.items():
                print_and_log(f'-- {runtime}: {pallets}')

    elif args.command == 'fmt':
        command = f"cargo +nightly fmt"
        print(f'Formatting with `{command}`')
        nightly_status = os.system(f'{command}')
        taplo_status = os.system('taplo format --config .config/taplo.toml')

        if (nightly_status != 0 or taplo_status != 0):
            print_and_log('❌ Failed to format code')
            sys.exit(1)

    elif args.command == 'update-ui':
        command = 'sh ./scripts/update-ui-tests.sh'
        print(f'Updating ui with `{command}`')
        status = os.system(f'{command}')

        if status != 0:
            print_and_log('❌ Failed to update ui')
            sys.exit(1)

    elif args.command == 'prdoc':
        # Call the main function from ./github/scripts/generate-prdoc.py module
        exit_code = generate_prdoc.main(args)
        if exit_code != 0:
            print_and_log('❌ Failed to generate prdoc')
            sys.exit(exit_code)

    print('🚀 Done')

if __name__ == '__main__':
    main()