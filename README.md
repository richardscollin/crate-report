# crate-report

A tool for analyzing unsafe code usage in Rust crates. It is a cli tool and a github action.

## Example usage

```bash
# CLI Installation
cargo install --git https://github.com/richardscollin/crate-report

# Analyze a crate
crate-report path/to/rust/crate

# Output report to CSV
crate-report --format csv --output baseline.csv

# Compare against baseline
crate-report --baseline baseline.csv
```

## GitHub Actions Integration

Crate a new workflow such as, `.github/workflows/crate-report.yml`, in the workflows directory.

```yaml
name: Crate Report

on:
  pull_request:
    branches: [ main ]
    paths:
      - '**/*.rs'

permissions:
  contents: read
  pull-requests: write

jobs:
  safety-analysis:
    runs-on: ubuntu-latest
    steps:
      - uses: richardscollin/crate-report@main
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Composite Action Inputs

| Input | Description | Required | Default |
|-------|-------------|----------|---------|
| `github-token` | GitHub token for commenting on PRs | Yes | `${{ github.token }}` |

## Output Format Examples

### CSV

```csv
filename,static_mut_items,total_fns,total_lines,total_statements,unsafe_fns,unsafe_statements,unwraps
src/main.rs,0,10,250,45,2,5,3
src/lib.rs,1,5,100,20,0,0,1
```

### Markdown

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
