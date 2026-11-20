# macOS tests

Run `just test` from the repository root. `just test core` runs the same macOS tests while omitting Rust GPU tests, for CI without a GPU.

The Command Line Tools toolchain does not include XCTest or Swift Testing. The test runner discovers `JamTests/*Tests.swift` and compiles them into one executable with assertions enabled. Each file defines a struct matching its filename and a `static func run() throws` entry point. Add new cases to these suites, or add a matching suite file; do not add individual just recipes.

The isolated app source dependencies are listed in `scripts/test.py`. These tests exercise application logic and AppKit layout without starting a daemon or opening the main app.
