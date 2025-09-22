# crate-report

A tool for analyzing unsafe code usage in Rust crates, providing detailed reports and tracking safety metrics over time.

## Features

- **Unsafe Code Analysis**: Detects unsafe functions, statements, static mut items, and unwrap calls
- **Multiple Output Formats**: CSV, Markdown, HTML, and PR-comment formats
- **Baseline Comparison**: Track changes over time with diff visualization
- **GitHub Integration**: Automated PR comments with safety analysis
- **CI/CD Ready**: GitHub Actions integration for continuous safety monitoring

## Installation

### From Source

```bash
git clone https://github.com/richardscollin/crate-report
cd crate-report
cargo install --path .
```

### From GitHub

```bash
cargo install --git https://github.com/richardscollin/crate-report crate-report
```

## Usage

### Basic Analysis

```bash
# Analyze current directory
crate-report

# Analyze specific crate directory
crate-report /path/to/rust/crate

# Generate CSV output
crate-report --format csv --output safety-report.csv

# Generate HTML report
crate-report --format html --output safety-report.html
```

### Baseline Comparison

```bash
# Generate baseline report
crate-report --format csv --output baseline.csv

# Compare against baseline
crate-report --baseline baseline.csv --format markdown
```

## GitHub Actions Integration

### Quick Setup

Add this workflow to your repository at `.github/workflows/safety-check.yml`:

```yaml
name: Safety Analysis

on:
  pull_request:
    branches: [ main ]
    paths:
      - '**/*.rs'
      - 'Cargo.toml'
      - 'Cargo.lock'

permissions:
  contents: read
  pull-requests: write

jobs:
  safety-analysis:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: richardscollin/crate-report/.github/actions/safety-check@main
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          crate-root: '.'
          base-branch: 'main'
```

### Composite Action Inputs

| Input | Description | Required | Default |
|-------|-------------|----------|---------|
| `github-token` | GitHub token for commenting on PRs | Yes | `${{ github.token }}` |
| `crate-root` | Root directory of the crate to analyze | No | `'.'` |
| `base-branch` | Base branch to compare against for baseline | No | `'main'` |
| `rust-toolchain` | Rust toolchain version to use | No | `'stable'` |

### Github Workflow Configuration

```yaml
- uses: richardscollin/crate-report/.github/actions/safety-check@main
  id: safety-check
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
    crate-root: './backend'
    base-branch: 'develop'
    rust-toolchain: '1.70.0'

- name: Upload reports
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: safety-reports
    path: |
      ${{ steps.safety-check.outputs.safety-report }}
      ${{ steps.safety-check.outputs.baseline-report }}
```

## Output Formats

### CSV Format

```csv
filename,static_mut_items,total_fns,total_lines,total_statements,unsafe_fns,unsafe_statements,unwraps
src/main.rs,0,10,250,45,2,5,3
src/lib.rs,1,5,100,20,0,0,1
```

### Markdown Format

```markdown
## Code Report

Total lines: 1250
Total unsafe functions: 15.2% (8 / 52)
Total statements in unsafe blocks: 23
Total static mut items: 2
Total unwrap calls: 12

| File | Unsafe/Total Functions | Statements | Static Mut | Unwraps |
|------|----------------------|------------|------------|---------|
| src/main.rs | 2/10 | 5/45 | 0 | 3 |
```

### HTML Format

Interactive report with sortable tables and visual metrics.

### PR Comment Format

GitHub PR comments with safety analysis:

```markdown
## Safety Analysis Report

### Summary

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Unsafe Functions | 2 | 0 | -2 |
| Unsafe Statements | 3 | 0 | -3 |
| Unwrap Calls | 18 | 22 | +4 |

**Mixed changes.** This PR has both safety improvements and regressions.
```

## Command Line Options

```
Usage: crate-report [OPTIONS] [CRATE_ROOT]

Arguments:
  [CRATE_ROOT]  Root directory of the crate to analyze [default: .]

Options:
  -b, --baseline <BASELINE>  Baseline CSV file to compare against
  -o, --output <OUTPUT>      Output file path (defaults to stdout)
  -f, --format <FORMAT>      Output format [default: markdown] [possible values: csv, html, markdown, pr-comment]
  -h, --help                 Print help
```

## Metrics Explained

- **Unsafe Functions**: Functions declared with the `unsafe` keyword
- **Unsafe Statements**: Individual statements within `unsafe` blocks
- **Static Mut Items**: Global mutable static variables (`static mut`)
- **Unwrap Calls**: Calls to `.unwrap()` method (potential panic points)
