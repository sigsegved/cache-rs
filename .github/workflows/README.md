# Rust CI Workflow Documentation

Rust CI workflow defined in [`rust.yml`](.github/workflows/rust.yml:1) automates building, testing, linting, formatting, coverage analysis, caching, artifact management, release automation, and supports multiple branches and Rust toolchains.

---

## Workflow Triggers

- **Push & Pull Requests:** Runs on pushes and PRs to `main`, `develop`, `release/*`, and `feature/*` branches.
- **Release Automation:** The release job runs only on `main` when the commit message starts with `release:` or `Release v`.

---

## Jobs Overview

### 1. Build & Test (Matrix) [`build-test`](.github/workflows/rust.yml:18)

- **Matrix Strategy:** Runs on Ubuntu, macOS, and Windows; Rust toolchains: stable, beta, nightly.
- **Steps:**
  - **Checkout code:** Uses `actions/checkout@v4`.
  - **Cache cargo registry:** Speeds up builds by caching dependencies.
  - **Install Rust toolchain:** Uses `actions-rs/toolchain@v1` for the selected matrix version.
  - **Build:** Runs `cargo build --verbose`.
  - **Test:** Runs `cargo test --all --verbose`.
  - **Upload test artifacts:** Stores test results for each OS/toolchain combination.

**Purpose:** Ensures code builds and passes tests across platforms and Rust versions.

---

### 2. Lint [`lint`](.github/workflows/rust.yml:60)

- **Runs on:** Ubuntu-latest.
- **Steps:**
  - **Checkout code**
  - **Install Rust toolchain (stable)**
  - **Run clippy:** Uses `actions-rs/clippy@v1` with strict warnings.

**Purpose:** Enforces code quality and style using Clippy linter.

---

### 3. Format [`format`](.github/workflows/rust.yml:79)

- **Runs on:** Ubuntu-latest.
- **Steps:**
  - **Checkout code**
  - **Install Rust toolchain (stable)**
  - **Check formatting:** Runs `cargo fmt --all -- --check`.

**Purpose:** Ensures code is properly formatted according to Rust standards.

---

### 4. Coverage [`coverage`](.github/workflows/rust.yml:96)

- **Runs on:** Ubuntu-latest.
- **Steps:**
  - **Checkout code**
  - **Install Rust toolchain (stable)**
  - **Install cargo-tarpaulin:** For coverage analysis.
  - **Run coverage:** Generates XML report.
  - **Upload coverage report:** Stores coverage results.
  - **Upload to Codecov:** Publishes coverage metrics to Codecov.

**Purpose:** Measures and reports code coverage.

---

### 5. Release [`release`](.github/workflows/rust.yml:127)

- **Runs on:** Ubuntu-latest, only on `main` branch with commit message starting with `release:` or `Release v`.
- **Steps:**
  - **Checkout code**
  - **Install Rust toolchain (stable)**
  - **Build release:** Runs `cargo build --release`.
  - **Publish to crates.io:** Publishes the crate to crates.io registry.
  - **Create GitHub Release:** Publishes binaries as a GitHub release.

**Purpose:** Automates release creation and artifact publishing.

---

## Key Features

- **Caching:** Uses [`actions/cache`](https://github.com/actions/cache) to speed up builds by caching dependencies.
- **Matrix Testing:** Ensures compatibility across OSes and Rust toolchains.
- **Artifact Upload:** Stores test results, coverage reports, and release binaries for later use.
- **Multi-Branch Support:** Runs on main, develop, release/*, and feature/* branches.
- **Release Automation:** Publishes releases only when intended, using commit message filters.
- **Coverage Reporting:** Integrates with Codecov for coverage metrics.

---

## Usage

- **CI runs automatically** on pushes and PRs to supported branches.
- **Release job** triggers only on main with a commit message starting with `release:` or `Release v`.
- **Artifacts** can be downloaded from the Actions tab for test results, coverage, and releases.

---

## References

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [actions-rs/toolchain](https://github.com/actions-rs/toolchain)
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin)
- [Codecov](https://about.codecov.io/)
