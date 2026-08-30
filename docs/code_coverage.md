# Code Coverage Integration for RustOS

This document describes how to integrate code coverage measurement into the CI pipeline.

## Overview

Code coverage is measured using `cargo-tarpaulin` for x86_64 targets and `grcov` for cross-platform coverage.

## Installation

### cargo-tarpaulin (Linux/x86_64)
```bash
cargo install cargo-tarpaulin
```

### grcov (Cross-platform)
```bash
cargo install grcov
```

## Running Coverage Locally

### Using tarpaulin
```bash
# Generate HTML report
cargo tarpaulin --output-dir ./coverage --out Html

# Generate XML for CI
cargo tarpaulin --output-dir ./coverage --out Xml

# Generate all formats
cargo tarpaulin --output-dir ./coverage --out Html --out Xml --out Lcov

# Run with specific features
cargo tarpaulin --features "std,alloc" --output-dir ./coverage --out Html

# Exclude test code from coverage
cargo tarpaulin --exclude-files tests/* --output-dir ./coverage --out Html
```

### Using grcov with llvm-cov
```bash
# Build with coverage instrumentation
RUSTFLAGS="-C instrument-coverage" \
LLVM_PROFILE_FILE="rustos-%p-%m.profraw" \
cargo test

# Generate report
grcov rustos-*.profraw -s . --binary-path ./target/debug/ \
    -t html --branch --ignore-not-existing -o ./coverage/html

# Generate lcov for Codecov
grcov rustos-*.profraw -s . --binary-path ./target/debug/ \
    -t lcov --branch --ignore-not-existing -o ./coverage/lcov.info
```

## CI Integration

The GitHub Actions workflow automatically:
1. Installs cargo-tarpaulin
2. Runs tests with coverage
3. Uploads coverage reports to Codecov.io

### Workflow Configuration

See `.github/workflows/regression-tests.yml` for the complete CI setup.

Key steps:
```yaml
- name: Install cargo-tarpaulin
  uses: actions-rs/install@v0.1
  with:
    crate: cargo-tarpaulin
    version: latest
    
- name: Run coverage
  run: cargo tarpaulin --timeout 120 --out Xml --out Lcov
  
- name: Upload to Codecov
  uses: codecov/codecov-action@v3
  with:
    files: ./cobertura.xml,./lcov.info
    flags: unittests
    fail_ci_if_error: false
```

## Coverage Thresholds

Target coverage levels:
- **Overall**: >80% line coverage
- **Critical modules** (security, memory, scheduler): >90%
- **Drivers**: >70%

Modules below threshold will generate warnings in CI.

## Interpreting Reports

### Line Coverage
Percentage of executable lines that were executed during tests.

### Branch Coverage
Percentage of control flow branches (if/else, match arms) that were taken.

### Function Coverage
Percentage of functions that were called at least once.

## Excluding Code from Coverage

Some code should be excluded from coverage metrics:

```rust
#[cfg_attr(tarpaulin, skip)]
fn debug_only_function() {
    // Debug code not tested
}

#[cfg(not(test))]
mod runtime_only {
    // Runtime code not suitable for unit tests
}
```

## Troubleshooting

### Tarpaulin fails with "unrecognized option"
Ensure you're using a compatible Rust nightly version:
```bash
rustup install nightly-2024-01-01
rustup default nightly-2024-01-01
```

### Low coverage on inline functions
Inline functions may not show correct coverage. Add `#[inline(never)]` for testing.

### Segmentation fault during coverage
Some kernel code may not be compatible with instrumentation. Exclude those modules:
```bash
cargo tarpaulin --exclude src/arch/* --exclude src/boot/*
```

## Generating Coverage Badge

Add to README.md:
```markdown
[![Coverage](https://codecov.io/gh/rust-os/rustos/branch/main/graph/badge.svg)](https://codecov.io/gh/rust-os/rustos)
```

## Best Practices

1. **Run coverage regularly**: Don't wait for CI to find coverage gaps
2. **Focus on critical paths**: Prioritize security and correctness-critical code
3. **Don't chase 100%**: Some code (panic handlers, OOM) is hard to test
4. **Use coverage as guide**: Not all uncovered code needs tests
5. **Combine with other metrics**: Coverage doesn't measure test quality

## References

- [cargo-tarpaulin documentation](https://github.com/xd009642/tarpaulin)
- [grcov documentation](https://github.com/mozilla/grcov)
- [Codecov documentation](https://docs.codecov.com)
- [Rust coverage book](https://rustc-dev-guide.rust-lang.org/coverage-instrumentation.html)
