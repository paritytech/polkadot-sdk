"""
This script is used to turn the JSON report produced by the revive differential tests tool into an
easy to consume markdown document for the purpose of reporting this information in the Polkadot SDK
CI. The full models used in the JSON report can be found in the revive differential tests repo and
the models used in this script are just a partial reproduction of the full report models.
"""

import json, typing, io, sys


class Report(typing.TypedDict):
    context: "Context"
    execution_information: dict["MetadataFilePathString", "MetadataFileReport"]


class MetadataFileReport(typing.TypedDict):
    case_reports: dict["CaseIdxString", "CaseReport"]


class CaseReport(typing.TypedDict):
    mode_execution_reports: dict["ModeString", "ExecutionReport"]


class ExecutionReport(typing.TypedDict):
    status: "TestCaseStatus"


class Context(typing.TypedDict):
    Test: "TestContext"


class TestContext(typing.TypedDict):
    corpus_configuration: "CorpusConfiguration"


class CorpusConfiguration(typing.TypedDict):
    test_specifiers: list["TestSpecifier"]


class CaseStatusSuccess(typing.TypedDict):
    status: typing.Literal["Succeeded"]
    steps_executed: int


class CaseStatusFailure(typing.TypedDict):
    status: typing.Literal["Failed"]
    reason: str


class CaseStatusIgnored(typing.TypedDict):
    status: typing.Literal["Ignored"]
    reason: str


TestCaseStatus = typing.Union[CaseStatusSuccess, CaseStatusFailure, CaseStatusIgnored]
"""A union type of all of the possible statuses that could be reported for a case."""

TestSpecifier = str
"""A test specifier string. For example resolc-compiler-tests/fixtures/solidity/test.json::0::Y+"""

ModeString = str
"""The mode string. For example Y+ >=0.8.13"""

MetadataFilePathString = str
"""The path to a metadata file. For example resolc-compiler-tests/fixtures/solidity/test.json"""

CaseIdxString = str
"""The index of a case as a string. For example '0'"""

PlatformString = typing.Union[
    typing.Literal["revive-dev-node-revm-solc"],
    typing.Literal["revive-dev-node-polkavm-resolc"],
]
"""A string of the platform on which the test was run"""


def path_relative_to_resolc_compiler_test_directory(path: str) -> str:
    """
    Given a path, this function returns the path relative to the resolc-compiler-test directory. The
    following is an example of an input and an output:

    Input: ~/polkadot-sdk/revive-differential-tests/resolc-compiler-tests/fixtures/solidity/test.json
    Output: test.json
    """

    return f"{path.split('resolc-compiler-tests/fixtures/solidity')[-1].strip('/')}"


def main() -> None:
    with open(sys.argv[1], "r") as file:
        report: Report = json.load(file)

    # Getting the platform string and resolving it into a simpler version of
    # itself.
    platform_identifier: PlatformString = typing.cast(PlatformString, sys.argv[2])
    if platform_identifier == "revive-dev-node-polkavm-resolc":
        platform: str = "PolkaVM"
    elif platform_identifier == "revive-dev-node-revm-solc":
        platform: str = "REVM"
    else:
        platform: str = platform_identifier

    # Starting the markdown document and adding information to it as we go.
    markdown_document: io.TextIOWrapper = open("report.md", "w")
    print(f"# Differential Tests Results ({platform})", file=markdown_document)

    # Getting all of the test specifiers from the report and making them relative to the tests dir.
    test_specifiers: list[str] = list(
        map(
            path_relative_to_resolc_compiler_test_directory,
            report["context"]["Test"]["corpus_configuration"]["test_specifiers"],
        )
    )
    print("## Specified Tests", file=markdown_document)
    for test_specifier in test_specifiers:
        print(f"* ``{test_specifier}``", file=markdown_document)

    # Counting the total number of test cases, successes, failures, and ignored tests
    total_number_of_cases: int = 0
    total_number_of_successes: int = 0
    total_number_of_failures: int = 0
    total_number_of_ignores: int = 0
    for _, mode_to_case_mapping in report["execution_information"].items():
        for _, case_idx_to_report_mapping in mode_to_case_mapping[
            "case_reports"
        ].items():
            for _, execution_report in case_idx_to_report_mapping[
                "mode_execution_reports"
            ].items():
                status: TestCaseStatus = execution_report["status"]

                total_number_of_cases += 1
                if status["status"] == "Succeeded":
                    total_number_of_successes += 1
                elif status["status"] == "Failed":
                    total_number_of_failures += 1
                elif status["status"] == "Ignored":
                    total_number_of_ignores += 1
                else:
                    raise Exception(
                        f"Encountered a status that's unknown to the script: {status}"
                    )

    print("## Counts", file=markdown_document)
    print(
        f"* **Total Number of Test Cases:** {total_number_of_cases}",
        file=markdown_document,
    )
    print(
        f"* **Total Number of Successes:** {total_number_of_successes}",
        file=markdown_document,
    )
    print(
        f"* **Total Number of Failures:** {total_number_of_failures}",
        file=markdown_document,
    )
    print(
        f"* **Total Number of Ignores:** {total_number_of_ignores}",
        file=markdown_document,
    )

    # Grouping the various test cases into dictionaries and groups depending on their status to make
    # them easier to include in the markdown document later on.
    successful_cases: dict[
        MetadataFilePathString, dict[CaseIdxString, set[ModeString]]
    ] = {}
    for metadata_file_path, mode_to_case_mapping in report[
        "execution_information"
    ].items():
        for case_idx_string, case_idx_to_report_mapping in mode_to_case_mapping[
            "case_reports"
        ].items():
            for mode_string, execution_report in case_idx_to_report_mapping[
                "mode_execution_reports"
            ].items():
                status: TestCaseStatus = execution_report["status"]
                metadata_file_path: str = (
                    path_relative_to_resolc_compiler_test_directory(metadata_file_path)
                )
                mode_string: str = mode_string.replace(" M3", "+").replace(" M0", "-")

                if status["status"] == "Succeeded":
                    successful_cases.setdefault(
                        metadata_file_path,
                        {},
                    ).setdefault(
                        case_idx_string, set()
                    ).add(mode_string)

    print("## Failures", file=markdown_document)
    print(
        "The test specifiers seen in this section have the format 'path::case_idx::compilation_mode'\
          and they're compatible with the revive differential tests framework and can be specified\
          to it directly in the same way that they're provided through the `--test` argument of the\
          framework.\n",
        file=markdown_document,
    )
    print(
        "The failures are provided in an expandable section to ensure that the PR does not get \
        polluted with information. Please click on the section below for more information",
        file=markdown_document,
    )
    print(
        "<details><summary>Detailed Differential Tests Failure Information</summary>\n\n",
        file=markdown_document,
    )
    print("| Test Specifier | Failure Reason | Note |", file=markdown_document)
    print("| -- | -- | -- |", file=markdown_document)

    for metadata_file_path, mode_to_case_mapping in report[
        "execution_information"
    ].items():
        for case_idx_string, case_idx_to_report_mapping in mode_to_case_mapping[
            "case_reports"
        ].items():
            for mode_string, execution_report in case_idx_to_report_mapping[
                "mode_execution_reports"
            ].items():
                status: TestCaseStatus = execution_report["status"]
                metadata_file_path: str = (
                    path_relative_to_resolc_compiler_test_directory(metadata_file_path)
                )
                mode_string: str = mode_string.replace(" M3", "+").replace(" M0", "-")

                if status["status"] != "Failed":
                    continue

                failure_reason: str = (
                    status["reason"].replace("\n", " ").replace("|", " ")
                )

                note: str = ""
                modes_where_this_case_succeeded: set[ModeString] = (
                    successful_cases.setdefault(
                        metadata_file_path,
                        {},
                    ).setdefault(case_idx_string, set())
                )
                if len(modes_where_this_case_succeeded) != 0:
                    note: str = (
                        f"This test case succeeded with other compilation modes: {modes_where_this_case_succeeded}"
                    )

                test_specifier: str = (
                    f"{metadata_file_path}::{case_idx_string}::{mode_string}"
                )
                print(
                    f"| ``{test_specifier}`` | ``{failure_reason}`` | {note} |",
                    file=markdown_document,
                )
    print("\n\n</details>", file=markdown_document)

    # The primary downside of not using `with`, but I guess it's better since I don't want to over
    # indent the code.
    markdown_document.close()


if __name__ == "__main__":
    main()
