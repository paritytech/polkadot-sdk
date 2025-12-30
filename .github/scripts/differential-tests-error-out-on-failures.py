"""
This script is used to turn the JSON report produced by the revive differential tests, detecting
failures and exiting with the appropriate code if a failure is encountered.
"""

import json, typing, sys


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


def main() -> None:
    with open(sys.argv[1], "r") as file:
        report: Report = json.load(file)

    for _, mode_to_case_mapping in report["execution_information"].items():
        for _, case_idx_to_report_mapping in mode_to_case_mapping[
            "case_reports"
        ].items():
            for _, execution_report in case_idx_to_report_mapping[
                "mode_execution_reports"
            ].items():
                if execution_report["status"]["status"] == "Failed":
                    exit(1)

    exit(0)


if __name__ == "__main__":
    main()
