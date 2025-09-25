mod bool_candidates;
mod html;
mod safe_candidates;

use std::{
    cmp,
    collections::{
        BTreeMap,
        BTreeSet,
    },
    iter::{
        Iterator,
        Sum,
    },
    path::Path,
};

use clap::CommandFactory;
use clap::Parser;
use colored::{
    Color,
    ColoredString,
    Colorize,
};
use syn::{
    ExprMethodCall,
    ExprUnsafe,
    ItemFn,
    ItemStatic,
    StaticMutability,
    Stmt,
    visit::Visit,
};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "crate-report")]
#[command(about = "Analyze unsafe code usage in Rust crates")]
struct Args {
    #[arg(help = "Root directory of the crate to analyze", default_value = ".")]
    crate_root: String,

    #[arg(long, help = "Baseline CSV file to compare against")]
    baseline: Option<String>,

    #[arg(long, short, help = "Output file path (defaults to stdout)")]
    output: Option<String>,

    #[arg(
        long,
        short,
        help = "Output format",
        value_enum,
        default_value = "markdown"
    )]
    format: OutputFormat,

    #[arg(long, default_value_t = false)]
    safe_candidates: bool,

    #[arg(long, default_value_t = false)]
    bool_candidates: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Csv,
    Html,
    Markdown,
    PrComment,
}

#[derive(Clone, Debug, Default)]
struct CodeStats {
    static_mut_items: isize,
    total_fns: isize,
    total_lines: isize,
    total_statements: isize,
    unsafe_fns: isize,
    unsafe_statements: isize,
    unwraps: isize,
}

#[derive(Clone)]
struct Report {
    files: BTreeMap<String, CodeStats>,
    total: CodeStats,
}

#[derive(Copy, Clone, Debug)]
struct Change<T> {
    after: T,
    before: T,
}

impl<T> Change<T> {
    fn project<U>(&self, f: impl Fn(&T) -> U) -> Change<U> {
        Change {
            after: f(&self.after),
            before: f(&self.before),
        }
    }
}

enum Diff {
    Added(CodeStats),
    Changed(Change<CodeStats>),
    Removed(CodeStats),
}

struct DiffReport {
    after_total: CodeStats,
    before_total: CodeStats,
    changes: BTreeMap<String /* filename */, Diff>,
}

impl DiffReport {
    fn color_display<W>(&self, mut out: W)
    where
        W: std::io::Write,
    {
        if self.changes.is_empty() {
            _ = writeln!(&mut out, "No changes");
        }

        // summary
        _ = writeln!(
            out,
            "Summary
=======
unsafe fn  : {}
total fn   : {}
total stmt : {}
static mut : {}
unwraps    : {}
",
            format_diff(
                self.before_total.unsafe_fns,
                self.after_total.unsafe_fns,
                DecreaseIs::Good
            ),
            format_diff(
                self.before_total.total_fns,
                self.after_total.total_fns,
                DecreaseIs::Neutral
            ),
            format_diff(
                self.before_total.unsafe_statements,
                self.after_total.unsafe_statements,
                DecreaseIs::Good
            ),
            format_diff(
                self.before_total.static_mut_items,
                self.after_total.static_mut_items,
                DecreaseIs::Good
            ),
            format_diff(
                self.before_total.unwraps,
                self.after_total.unwraps,
                DecreaseIs::Good
            ),
        );

        // print in order: changed, added, removed

        for (filename, diff) in &self.changes {
            if let Diff::Changed(change) = diff {
                let unsafe_fns = change.project(|e| e.unsafe_fns);
                let total_fns = change.project(|e| e.total_fns);

                _ = writeln!(
                    out,
                    "{filename}
unsafe fn   : {}
unsafe stmt : {}
static mut  : {}
unwraps     : {}
",
                    format_unsafe_fn_change(unsafe_fns, total_fns),
                    format_diff(
                        change.before.unsafe_statements,
                        change.after.unsafe_statements,
                        DecreaseIs::Good
                    ),
                    format_diff(
                        change.before.static_mut_items,
                        change.after.static_mut_items,
                        DecreaseIs::Good
                    ),
                    format_diff(
                        change.before.unwraps,
                        change.after.unwraps,
                        DecreaseIs::Good
                    ),
                );
            }
        }

        for (filename, diff) in &self.changes {
            if let Diff::Added(CodeStats {
                unsafe_fns,
                total_fns,
                unsafe_statements,
                unwraps,
                ..
            }) = diff
            {
                _ = writeln!(
                    out,
                    "{filename} [NEW FILE]
  Unsafe funcs: {unsafe_fns}
   Total funcs: {total_fns}
  Unsafe stmts: {unsafe_statements}
       unwraps: {unwraps}
"
                );
            }
        }

        for (filename, diff) in &self.changes {
            if let Diff::Removed(CodeStats {
                unsafe_fns,
                total_fns,
                unsafe_statements,
                ..
            }) = diff
            {
                _ = writeln!(
                    out,
                    "{filename} [REMOVED]
  Had {unsafe_fns} unsafe / {total_fns} total fns, {unsafe_statements} unsafe lines\n"
                );
            }
        }
    }
}

impl Report {
    fn diff(&self, baseline: &Self) -> DiffReport {
        let all_files: BTreeSet<&str> = baseline
            .files
            .keys()
            .chain(self.files.keys())
            .map(|e| e.as_str())
            .collect();

        DiffReport {
            after_total: self.total.clone(),
            before_total: baseline.total.clone(),

            changes: all_files
                .into_iter()
                .flat_map(|filename| {
                    match (
                        baseline.files.get(filename).cloned(),
                        self.files.get(filename).cloned(),
                    ) {
                        (Some(before), Some(after)) if before.should_report_change(&after) => {
                            Some((
                                filename.to_string(),
                                Diff::Changed(Change { before, after }),
                            ))
                        }
                        (None, Some(new)) => Some((filename.to_string(), Diff::Added(new))),
                        (Some(old), None) => Some((filename.to_string(), Diff::Removed(old))),
                        (_, _) => None,
                    }
                })
                .collect(),
        }
    }

    fn to_table(&self) -> Table<5> {
        let mut table = Table::with_headers([
            "".into(),
            " (unsafe/total) fns".into(),
            "statements".into(),
            "static mut".into(),
            "unwrap".into(),
        ]);
        table.extend_rows(self.files.iter().map(|(filename, file_report)| {
            [
                style_filename(filename, file_report), // filename
                colorize_ratio(file_report.unsafe_fns, file_report.total_fns), // unsafe fns
                format!(
                    "{}/{}",
                    file_report.unsafe_statements, file_report.total_statements
                )
                .into(), // unsafe statements
                colorize_simple(file_report.static_mut_items), // static mut
                colorize_simple(file_report.unwraps),  // unwraps
            ]
        }));
        table
    }
}

impl CodeStats {
    fn is_perfect(&self) -> bool {
        self.unsafe_fns == 0
            && self.unsafe_statements == 0
            && self.static_mut_items == 0
            && self.unwraps == 0
    }

    fn should_report_change(&self, rhs: &Self) -> bool {
        let Self {
            total_fns: _,        // ignore
            total_statements: _, // ignore
            total_lines: _,      // ignore

            unsafe_fns,
            unsafe_statements,
            static_mut_items,
            unwraps,
        } = rhs;

        self.unsafe_fns != *unsafe_fns
            || self.unsafe_statements != *unsafe_statements
            || self.static_mut_items != *static_mut_items
            || self.unwraps != *unwraps
    }

    fn from_csv_row(value: &[&str; 8]) -> Option<(String, Self)> {
        let [
            filename,
            static_mut_items,
            total_fns,
            total_lines,
            total_statements,
            unsafe_fns,
            unsafe_statements,
            unwraps,
        ] = value;

        Some((
            filename.to_string(),
            Self {
                static_mut_items: static_mut_items.parse().ok()?,
                total_fns: total_fns.parse().ok()?,
                total_lines: total_lines.parse().ok()?,
                total_statements: total_statements.parse().ok()?,
                unsafe_fns: unsafe_fns.parse().ok()?,
                unsafe_statements: unsafe_statements.parse().ok()?,
                unwraps: unwraps.parse().ok()?,
            },
        ))
    }

    fn csv_headers() -> [String; 8] {
        [
            "filename".to_string(),
            "static_mut_items".into(),
            "total_fns".into(),
            "total_lines".into(),
            "total_statements".into(),
            "unsafe_fns".into(),
            "unsafe_statements".into(),
            "unwraps".into(),
        ]
    }

    fn to_csv_row(&self, filename: String) -> [String; 8] {
        [
            filename,
            self.static_mut_items.to_string(),
            self.total_fns.to_string(),
            self.total_lines.to_string(),
            self.total_statements.to_string(),
            self.unsafe_fns.to_string(),
            self.unsafe_statements.to_string(),
            self.unwraps.to_string(),
        ]
    }
}

impl Sum for CodeStats {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(
            |mut acc,
             CodeStats {
                 static_mut_items,
                 total_fns,
                 total_lines,
                 total_statements,
                 unsafe_fns,
                 unsafe_statements,
                 unwraps,
             }| {
                acc.static_mut_items += static_mut_items;
                acc.static_mut_items += static_mut_items;
                acc.total_fns += total_fns;
                acc.total_lines += total_lines;
                acc.total_statements += total_statements;
                acc.unsafe_fns += unsafe_fns;
                acc.unsafe_statements += unsafe_statements;
                acc.unwraps += unwraps;
                acc
            },
        )
        .unwrap_or_default()
    }
}

struct CodeAnalyzer<'a> {
    stats: &'a mut CodeStats,
}
impl<'a, 'ast> Visit<'ast> for CodeAnalyzer<'a> {
    fn visit_expr_method_call(&mut self, i: &'ast ExprMethodCall) {
        if i.method == "unwrap" {
            self.stats.unwraps += 1;
        }
        syn::visit::visit_expr_method_call(self, i);
    }

    fn visit_expr_unsafe(&mut self, i: &'ast ExprUnsafe) {
        self.stats.unsafe_statements += i.block.stmts.len() as isize;
        syn::visit::visit_expr_unsafe(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        self.stats.total_fns += 1;
        if i.sig.unsafety.is_some() {
            self.stats.unsafe_fns += 1;
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_item_static(&mut self, i: &'ast ItemStatic) {
        if !matches!(i.mutability, StaticMutability::None) {
            self.stats.static_mut_items += 1;
        }
        syn::visit::visit_item_static(self, i);
    }

    fn visit_stmt(&mut self, i: &'ast Stmt) {
        self.stats.total_statements += 1;
        syn::visit::visit_stmt(self, i);
    }
}

fn analyze_file(path: &Path) -> Option<CodeStats> {
    let content = std::fs::read_to_string(path).ok()?;
    let syntax = syn::parse_file(&content).ok()?;

    let mut stats = CodeStats {
        total_lines: content.lines().count() as isize,
        ..CodeStats::default()
    };

    let mut visitor = CodeAnalyzer { stats: &mut stats };
    visitor.visit_file(&syntax);

    Some(stats)
}

fn generate_report(root: &str) -> Report {
    let root_path = Path::new(root);
    let file_paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map(|s| s != "target")
                .unwrap_or(true)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "rs").unwrap_or(false))
        .collect();

    let analyze_path = |e: &walkdir::DirEntry| {
        let path = e.path();
        let stats = analyze_file(path)?;
        let relative_path = path
            .strip_prefix(root_path)
            .expect("must start with root prefix while walking dir");
        Some((relative_path.display().to_string(), stats))
    };

    #[cfg(feature = "rayon")]
    use rayon::prelude::*;
    #[cfg(feature = "rayon")]
    let file_reports = file_paths
        .par_iter()
        .flat_map(analyze_path)
        .collect::<BTreeMap<String, CodeStats>>();

    #[cfg(not(feature = "rayon"))]
    let file_reports = file_paths
        .iter()
        .flat_map(analyze_path)
        .collect::<BTreeMap<String, CodeStats>>();

    Report {
        total: file_reports.values().cloned().sum(),
        files: file_reports,
    }
}

enum DecreaseIs {
    Good,
    Neutral,
}
fn format_diff(old: isize, new: isize, decrease_is: DecreaseIs) -> String {
    let delta = new - old;

    if delta == 0 {
        return format!("{old} (no change)")
            .color(Color::BrightBlack)
            .to_string();
    }

    let plus = if delta > 0 { "+" } else { "" };
    let color = match decrease_is {
        DecreaseIs::Neutral => Color::BrightBlack,
        DecreaseIs::Good => {
            if delta > 0 {
                Color::Red
            } else if delta < 0 {
                Color::Green
            } else {
                Color::BrightBlack
            }
        }
    };

    format!("{old} -> {new} ({plus}{delta})")
        .color(color)
        .to_string()
}

fn format_unsafe_fn_change(unsafe_fn: Change<isize>, total_fn: Change<isize>) -> String {
    let unsafe_lines_changed = unsafe_fn.after - unsafe_fn.before;
    let total_lines_changed = total_fn.after - total_fn.before;

    if unsafe_lines_changed == 0 && total_lines_changed == 0 {
        return format!("{}/{} (no change)", unsafe_fn.after, total_fn.after)
            .color(Color::White)
            .to_string();
    }

    let (sign, color) = match unsafe_lines_changed.cmp(&0) {
        cmp::Ordering::Less => ("-", Color::Green),
        cmp::Ordering::Greater => ("+", Color::Red),
        cmp::Ordering::Equal => ("", Color::White),
    };

    format!(
        "{}/{} -> {}/{} ({sign}{})",
        unsafe_fn.before,
        total_fn.before,
        unsafe_fn.after,
        total_fn.after,
        unsafe_lines_changed.abs()
    )
    .color(color)
    .to_string()
}

fn style_filename(filename: &str, stats: &CodeStats) -> ColoredString {
    if stats.is_perfect() {
        filename.color(Color::Green)
    } else {
        filename.into()
    }
}

fn colorize_percentage(unsafe_count: isize, total_count: isize) -> ColoredString {
    let color = if total_count == 0 {
        Color::BrightBlack
    } else if unsafe_count == 0 {
        Color::Green
    } else if (unsafe_count as f64 / total_count as f64) < 0.5 {
        Color::Yellow
    } else {
        Color::Red
    };

    let percentage = if total_count == 0 {
        0.0
    } else {
        (unsafe_count as f64 / total_count as f64) * 100.0
    };

    format!("{percentage:.02}% ({unsafe_count} / {total_count})").color(color)
}

fn colorize_ratio(unsafe_count: isize, total_count: isize) -> ColoredString {
    let color = if total_count == 0 {
        Color::BrightBlack
    } else if unsafe_count == 0 {
        Color::Green
    } else if (unsafe_count as f64 / total_count as f64) < 0.5 {
        Color::Yellow
    } else {
        Color::Red
    };

    format!("{unsafe_count}/{total_count}").color(color)
}

/// colorize such that zero is green, single digit is yellow, more then that is red
fn colorize_simple(count: isize) -> ColoredString {
    let color = if count == 0 {
        Color::Green
    } else if count < 10 {
        Color::Yellow
    } else {
        Color::Red
    };

    count.to_string().color(color)
}

fn main() {
    let args = Args::parse();

    // Sanity check: ensure Cargo.toml exists in the crate root
    let crate_root_path = Path::new(&args.crate_root);
    let cargo_toml_path = crate_root_path.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        let mut cmd = Args::command();
        let expanded_path = crate_root_path
            .canonicalize()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| args.crate_root.clone());
        eprintln!("Error: No Cargo.toml found in '{}'", expanded_path);
        eprintln!("Please specify a valid Rust crate directory.");
        eprintln!();
        _ = cmd.print_help();
        return;
    }

    if args.safe_candidates {
        let stats = safe_candidates::find_candidates(crate_root_path);

        if !stats.is_empty() {
            println!("These candidates are chosen using a very simple heuristic.
If a function is unsafe and has no raw pointers as parameters, it may be a good candidate for making safe.
Note that there may be other reasons why these functions shouldn't be converted.
");

            let file_count = stats.len();
            let candidates_count: usize = stats.iter().map(|e| e.stats.candidates.len()).sum();

            for stat in stats {
                let safe_candidates::FileStats {
                    filename,
                    stats: code_stats,
                } = stat;

                println!("{filename}:");
                for candidate in code_stats.candidates {
                    println!(
                        "\t{} @ {}:{}",
                        candidate.fn_name, filename, candidate.line_number
                    );
                }
            }
            println!(
                "\nFound {} candidates over {} files (more files total)",
                candidates_count, file_count,
            );
        } else {
            println!(
                "No candidates found for functions to convert from unsafe to safe using a simple heuristic."
            )
        }
        return;
    }

    if args.bool_candidates {
        let stats = bool_candidates::find_candidates(crate_root_path);

        if !stats.is_empty() {
            println!("These candidates are chosen using a very simple heuristic.
If a function returns i32 and all return statements return literal 0 or 1 values, it may be a good candidate for converting to return bool.
Note that there may be other reasons why these functions shouldn't be converted.
");

            let file_count = stats.len();
            let candidates_count: usize = stats.iter().map(|e| e.stats.candidates.len()).sum();

            for stat in stats {
                let bool_candidates::FileStats {
                    filename,
                    stats: code_stats,
                } = stat;

                println!("{filename}:");
                for candidate in code_stats.candidates {
                    println!(
                        "\t{} @ {}:{}",
                        candidate.fn_name, filename, candidate.line_number
                    );
                }
            }
            println!(
                "\nFound {} candidates over {} files (more files total)",
                candidates_count, file_count,
            );
        } else {
            println!(
                "No candidates found for functions to convert from i32 to bool using a simple heuristic."
            )
        }
        return;
    }

    let report = generate_report(&args.crate_root);

    // Handle output based on format
    match args.format {
        OutputFormat::Csv => {
            let mut writer = csv::WriterBuilder::new().from_writer(std::io::BufWriter::new(
                if let Some(output_file) = &args.output {
                    Box::new(std::fs::File::create(output_file).unwrap()) as Box<dyn std::io::Write>
                } else {
                    Box::new(std::io::stdout()) as Box<dyn std::io::Write>
                },
            ));

            _ = writer.serialize(CodeStats::csv_headers());
            for (filename, code_stats) in report.files.iter() {
                _ = writer.serialize(code_stats.to_csv_row(filename.to_string()));
            }
        }
        OutputFormat::Html => {
            let output_content = html::format_html_report(&report, &args);
            if let Some(output_file) = &args.output {
                std::fs::write(output_file, output_content).unwrap();
            } else {
                println!();
                print!("{}", output_content);
            }
        }
        OutputFormat::Markdown => {
            if let Some(output_file) = &args.output {
                // Disable colors when writing to file
                colored::control::set_override(false);
                let output_content = format_markdown_report(&report, &args);
                std::fs::write(output_file, output_content).unwrap();
                // Re-enable colors for any subsequent output
                colored::control::unset_override();
            } else {
                let output_content = format_markdown_report(&report, &args);
                println!("\n{output_content}");
            }
        }
        OutputFormat::PrComment => {
            let output_content = format_pr_comment_report(&report, &args);
            if let Some(output_file) = &args.output {
                std::fs::write(output_file, output_content).unwrap();
            } else {
                print!("{}", output_content);
            }
        }
    }
}

fn format_markdown_report(report: &Report, args: &Args) -> String {
    let mut out = Vec::<u8>::new();

    let CodeStats {
        total_lines,
        unsafe_statements,
        static_mut_items,
        unwraps,
        ..
    } = report.total;
    out.extend(
        format!(
            "Code Report
===========
- Total lines: {total_lines}
- Total unsafe functions: {}
- Total statements in unsafe blocks: {unsafe_statements}
- Total static mut items: {static_mut_items}
- Total unwrap calls: {unwraps}

",
            colorize_percentage(report.total.unsafe_fns, report.total.total_fns)
        )
        .bytes(),
    );
    report.to_table().to_markdown(&mut out);

    if let Some(baseline_file) = &args.baseline {
        let mut reader = csv::Reader::from_path(baseline_file).unwrap();

        // Validate CSV headers
        let headers: Vec<String> = reader
            .headers()
            .unwrap()
            .into_iter()
            .map(|h| h.to_string())
            .collect();
        assert_eq!(
            headers,
            CodeStats::csv_headers(),
            "CSV headers do not match expected format"
        );

        let files = reader
            .records()
            .map(|result| {
                let record = result.unwrap();
                let row: [&str; 8] = record.deserialize(None).unwrap();

                CodeStats::from_csv_row(&row).unwrap()
            })
            .collect::<BTreeMap<String, CodeStats>>();
        let old_report = Report {
            total: files.values().cloned().sum(),
            files,
        };

        out.extend("\n\n".bytes());
        report.diff(&old_report).color_display(&mut out);
    }

    out.extend(
        "\nGenerated by [crate-report](https://github.com/richardscollin/crate-report)\n".bytes(),
    );
    String::from_utf8(out).unwrap()
}

fn format_pr_comment_report(report: &Report, args: &Args) -> String {
    // If no baseline provided, don't generate PR comment
    let Some(baseline_file) = &args.baseline else {
        return String::new();
    };

    // Load baseline data
    let mut reader = match csv::Reader::from_path(baseline_file) {
        Ok(reader) => reader,
        Err(_) => return String::new(),
    };

    // Validate CSV headers
    let headers: Vec<String> = match reader.headers() {
        Ok(headers) => headers.into_iter().map(|h| h.to_string()).collect(),
        Err(_) => return String::new(),
    };

    if headers != CodeStats::csv_headers() {
        return String::new();
    }

    // Parse baseline data
    let files = reader
        .records()
        .filter_map(|result| {
            let record = result.ok()?;
            let row: [&str; 8] = record.deserialize(None).ok()?;
            CodeStats::from_csv_row(&row)
        })
        .collect::<BTreeMap<String, CodeStats>>();

    let old_report = Report {
        total: files.values().cloned().sum(),
        files,
    };

    let diff = report.diff(&old_report);

    // If no changes, generate a "no changes" comment
    if diff.changes.is_empty() {
        return format!(
            "## Safety Analysis Report\n\n\
             **No safety changes detected.** This PR doesn't modify any safety-related metrics.\n\n\
             | Metric | Current |\n\
             |--------|--------|\n\
             | Unsafe Functions | {} |\n\
             | Unsafe Statements | {} |\n\
             | Static Mut Items | {} |\n\
             | Unwrap Calls | {} |\n\n\
             ---\n\
             *Generated by [crate-report](https://github.com/richardscollin/crate-report)*",
            diff.after_total.unsafe_fns,
            diff.after_total.unsafe_statements,
            diff.after_total.static_mut_items,
            diff.after_total.unwraps
        );
    }

    let mut out = String::new();

    // Header
    out.push_str("## Crate Report\n\n");

    // Summary section
    let unsafe_fn_delta = diff.after_total.unsafe_fns - diff.before_total.unsafe_fns;
    let unsafe_stmt_delta =
        diff.after_total.unsafe_statements - diff.before_total.unsafe_statements;
    let static_mut_delta = diff.after_total.static_mut_items - diff.before_total.static_mut_items;
    let unwrap_delta = diff.after_total.unwraps - diff.before_total.unwraps;

    out.push_str("### Summary\n\n");
    out.push_str(&format!(
        "| Metric | Before | After | Change |\n\
         |--------|--------|-------|--------|\n\
         | Unsafe Functions | {} | {} | {} |\n\
         | Unsafe Statements | {} | {} | {} |\n\
         | Static Mut Items | {} | {} | {} |\n\
         | Unwrap Calls | {} | {} | {} |\n\n",
        diff.before_total.unsafe_fns,
        diff.after_total.unsafe_fns,
        format_pr_delta(unsafe_fn_delta),
        diff.before_total.unsafe_statements,
        diff.after_total.unsafe_statements,
        format_pr_delta(unsafe_stmt_delta),
        diff.before_total.static_mut_items,
        diff.after_total.static_mut_items,
        format_pr_delta(static_mut_delta),
        diff.before_total.unwraps,
        diff.after_total.unwraps,
        format_pr_delta(unwrap_delta)
    ));

    // Overall assessment
    let total_negative_changes = [
        unsafe_fn_delta,
        unsafe_stmt_delta,
        static_mut_delta,
        unwrap_delta,
    ]
    .iter()
    .filter(|&&x| x > 0)
    .count();

    let total_positive_changes = [
        unsafe_fn_delta,
        unsafe_stmt_delta,
        static_mut_delta,
        unwrap_delta,
    ]
    .iter()
    .filter(|&&x| x < 0)
    .count();

    if total_negative_changes == 0 && total_positive_changes > 0 {
        out.push_str("This PR reduces unsafe code usage.\n\n");
    } else if total_negative_changes > 0 && total_positive_changes == 0 {
        out.push_str("This PR introduces more unsafe code.\n\n");
    } else if total_negative_changes > 0 && total_positive_changes > 0 {
        out.push_str("This PR has both quality improvements and regressions.\n\n");
    } else {
        out.push_str(
            "**No safety changes.** File changes detected but no impact on quality metrics.\n\n",
        );
    }

    // Detailed changes (collapsible if many changes)
    if diff.changes.len() > 5 {
        out.push_str("<details>\n<summary>Detailed File Changes</summary>\n\n");
    } else {
        out.push_str("### File Changes\n\n");
    }

    for (filename, change) in &diff.changes {
        match change {
            Diff::Added(stats) => {
                out.push_str(&format!(
                    "- **{}** [NEW]\n  - Unsafe functions: {}, Statements: {}, Unwraps: {}\n",
                    filename, stats.unsafe_fns, stats.unsafe_statements, stats.unwraps
                ));
            }
            Diff::Removed(stats) => {
                out.push_str(&format!(
                    "- **{}** [REMOVED]\n  - Had: {} unsafe functions, {} statements, {} unwraps\n",
                    filename, stats.unsafe_fns, stats.unsafe_statements, stats.unwraps
                ));
            }
            Diff::Changed(change) => {
                let mut changes = Vec::new();
                if change.before.unsafe_fns != change.after.unsafe_fns {
                    changes.push(format!(
                        "unsafe functions: {} → {}",
                        change.before.unsafe_fns, change.after.unsafe_fns
                    ));
                }
                if change.before.unsafe_statements != change.after.unsafe_statements {
                    changes.push(format!(
                        "unsafe statements: {} → {}",
                        change.before.unsafe_statements, change.after.unsafe_statements
                    ));
                }
                if change.before.unwraps != change.after.unwraps {
                    changes.push(format!(
                        "unwraps: {} → {}",
                        change.before.unwraps, change.after.unwraps
                    ));
                }

                if !changes.is_empty() {
                    out.push_str(&format!(
                        "- **{}** [MODIFIED]\n  - {}\n",
                        filename,
                        changes.join(", ")
                    ));
                }
            }
        }
    }

    if diff.changes.len() > 5 {
        out.push_str("\n</details>\n");
    }

    out.push_str(
        "\n---\n*Generated by [crate-report](https://github.com/richardscollin/crate-report)*",
    );

    out
}

fn format_pr_delta(delta: isize) -> String {
    match delta {
        0 => "0".to_string(),
        x if x > 0 => format!("+{}", x),
        x => format!("{}", x),
    }
}

fn format_change_delta(before: isize, after: isize) -> String {
    let delta = after - before;
    if delta == 0 {
        "no change".to_string()
    } else if delta > 0 {
        format!("+{}", delta)
    } else {
        delta.to_string()
    }
}

/// A helper for displaying a table of data
struct Table<const N: usize> {
    headers: [ColoredString; N],
    rows: Vec<[ColoredString; N]>,
}
impl<const N: usize> Table<N> {
    fn with_headers(headers: [ColoredString; N]) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    fn extend_rows<I>(&mut self, rows: I)
    where
        I: Iterator<Item = [ColoredString; N]>,
    {
        self.rows.extend(rows)
    }

    fn to_markdown<W>(&self, mut out: W)
    where
        W: std::io::Write,
    {
        let rows = Some(&self.headers).into_iter().chain(&self.rows);

        let mut column_widths = vec![0; N];
        for row in rows.clone() {
            for (c, text) in row.iter().enumerate() {
                column_widths[c] = column_widths[c].max(text.len());
            }
        }

        // headers
        {
            let mut it = self.headers.iter().zip(&column_widths);

            // left align first column
            let (col, width) = it.next().unwrap();
            _ = write!(&mut out, "| {col:<width$} | ");

            // right align other columns
            for (col, width) in it {
                _ = write!(&mut out, " {col:>width$} |");
            }
            _ = writeln!(&mut out);
        }

        // "| -- | -: | -: | -: | -: |\n"
        {
            let mut it = column_widths.iter();
            let width = it.next().unwrap();
            _ = write!(&mut out, "| {:-<width$} | ", ":");

            // right align other columns
            for width in it {
                _ = write!(&mut out, " {:->width$} |", ":");
            }
            _ = writeln!(&mut out);
        }

        for row in &self.rows {
            let mut it = row.iter().zip(&column_widths);

            // left align first column
            let (col, width) = it.next().unwrap();
            _ = write!(&mut out, "| {col:<width$} | ");

            // right align other columns
            for (col, width) in it {
                _ = write!(&mut out, " {col:>width$} |");
            }
            _ = writeln!(&mut out);
        }
    }
}
