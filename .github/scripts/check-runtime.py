#!/usr/bin/env python3

import json
import sys
import logging
import os


def check_constant(spec_pallet_id, spec_pallet_value, meta_constant):
    """
    Check a single constant

    :param spec_pallet_id:
    :param spec_pallet_value:
    :param meta_constant:
    :return:
    """
    if meta_constant['name'] == list(spec_pallet_value.keys())[0]:
        constant = meta_constant['name']
        res = list(spec_pallet_value.values())[0]["value"] == meta_constant["value"]

        logging.debug(f"  Checking pallet:{spec_pallet_id}/constants/{constant}")
        logging.debug(f"    spec_pallet_value: {spec_pallet_value}")
        logging.debug(f"    meta_constant: {meta_constant}")
        logging.info(f"pallet:{spec_pallet_id}/constants/{constant} -> {res}")
        return res
    else:
        # logging.warning(f"  Skipping pallet:{spec_pallet_id}/constants/{meta_constant['name']}")
        pass


def check_pallet(metadata, spec_pallet):
    """
    Check one pallet

    :param metadata:
    :param spec_pallet_id:
    :param spec_pallet_value:
    :return:
    """

    spec_pallet_id, spec_pallet_value = spec_pallet
    logging.debug(f"Pallet: {spec_pallet_id}")

    metadata_pallets = metadata["pallets"]
    metadata_pallet = metadata_pallets[spec_pallet_id]

    res = map(lambda meta_constant_value: check_constant(
        spec_pallet_id, spec_pallet_value["constants"], meta_constant_value),
              metadata_pallet["constants"].values())
    res = list(filter(lambda item: item is not None, res))
    return all(res)


def check_pallets(metadata, specs):
    """
    CHeck all pallets

    :param metadata:
    :param specs:
    :return:
    """

    res = list(map(lambda spec_pallet: check_pallet(metadata, spec_pallet),
                   specs['pallets'].items()))
    res = list(filter(lambda item: item is not None, res))
    return all(res)


def check_metadata(metadata, specs):
    """
    Check metadata (json) against a list of expectations

    :param metadata: Metadata in JSON format
    :param expectation: Expectations
    :return: Bool
    """

    res = check_pallets(metadata, specs)
    return res


def help():
    """ Show some simple help """

    print(f"You must pass 2 args, you passed {len(sys.argv) - 1}")
    print("Sample call:")
    print("check-runtime.py <metadata.json> <specs.json>")


def load_json(file):
    """ Load json from a file """

    f = open(file)
    return json.load(f)


def main():
    LOGLEVEL = os.environ.get('LOGLEVEL', 'INFO').upper()
    logging.basicConfig(level=LOGLEVEL)

    if len(sys.argv) != 3:
        help()
        exit(1)

    metadata_file = sys.argv[1]
    specs_file = sys.argv[2]
    print(f"Checking metadata from: {metadata_file} with specs from: {specs_file}")

    metadata = load_json(metadata_file)
    specs = load_json(specs_file)

    res = check_metadata(metadata, specs)

    if res:
        logging.info(f"OK")
        exit(0)
    else:
        print("")
        logging.info(f"Some errors were found, run again with LOGLEVEL=debug")
        exit(1)

if __name__ == "__main__":
    main()
