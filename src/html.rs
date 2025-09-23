use crate::{
    Args,
    CodeStats,
    Diff,
    DiffReport,
    Report,
    format_change_delta,
};

pub fn format_html_report(report: &Report, args: &Args) -> String {
    let mut html = String::new();

    // HTML document structure with embedded CSS
    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Crate Safety Report</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; background: #f8f9fa; }
        .container { max-width: 1200px; margin: 0 auto; padding: 20px; }
        .header { background: white; border-radius: 8px; padding: 30px; margin-bottom: 30px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .header h1 { color: #2c3e50; margin-bottom: 10px; }
        .header .subtitle { color: #7f8c8d; }
        .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; margin-bottom: 30px; }
        .metric { background: white; border-radius: 8px; padding: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); text-align: center; }
        .metric-value { font-size: 2em; font-weight: bold; margin-bottom: 5px; }
        .metric-label { color: #7f8c8d; font-size: 0.9em; }
        .safe { color: #27ae60; }
        .warning { color: #f39c12; }
        .danger { color: #e74c3c; }
        .neutral { color: #7f8c8d; }
        table { width: 100%; background: white; border-radius: 8px; overflow: hidden; box-shadow: 0 2px 4px rgba(0,0,0,0.1); border-collapse: collapse; }
        th, td { padding: 12px 15px; text-align: left; border-bottom: 1px solid #ecf0f1; }
        th { background: #34495e; color: white; font-weight: 600; position: sticky; top: 0; cursor: pointer; user-select: none; }
        th:hover { background: #2c3e50; }
        tr:hover { background: #f8f9fa; }
        .perfect-file { color: #27ae60 !important; }
        .diff-section { background: white; border-radius: 8px; padding: 20px; margin-top: 30px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .diff-summary { margin-bottom: 20px; }
        .diff-change { margin: 10px 0; padding: 10px; border-radius: 4px; background: #f8f9fa; }
        .sortable { position: relative; }
        .sortable:after { content: ' ↕'; opacity: 0.5; }
        .sort-asc:after { content: ' ↑'; opacity: 1; }
        .sort-desc:after { content: ' ↓'; opacity: 1; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🦀 Crate Safety Report</h1>
            <div class="subtitle">Analysis of unsafe code usage in Rust crate</div>
        </div>
"#);

    // Summary metrics
    let CodeStats {
        total_lines,
        total_fns,
        unsafe_fns,
        unsafe_statements,
        static_mut_items,
        unwraps,
        ..
    } = &report.total;

    let unsafe_fn_percentage = if *total_fns > 0 {
        (*unsafe_fns as f64 / *total_fns as f64) * 100.0
    } else {
        0.0
    };

    html.push_str(&format!(
        r#"
        <div class="summary">
            <div class="metric">
                <div class="metric-value neutral">{}</div>
                <div class="metric-label">Total Lines</div>
            </div>
            <div class="metric">
                <div class="metric-value {}">{:.1}%</div>
                <div class="metric-label">Unsafe Functions</div>
            </div>
            <div class="metric">
                <div class="metric-value {}">{}</div>
                <div class="metric-label">Unsafe Statements</div>
            </div>
            <div class="metric">
                <div class="metric-value {}">{}</div>
                <div class="metric-label">Static Mut Items</div>
            </div>
            <div class="metric">
                <div class="metric-value {}">{}</div>
                <div class="metric-label">Unwrap Calls</div>
            </div>
        </div>
"#,
        total_lines,
        get_safety_class(*unsafe_fns, *total_fns),
        unsafe_fn_percentage,
        get_count_class(*unsafe_statements),
        unsafe_statements,
        get_count_class(*static_mut_items),
        static_mut_items,
        get_count_class(*unwraps),
        unwraps
    ));

    // File details table
    html.push_str(
        r#"
        <table id="fileTable">
            <thead>
                <tr>
                    <th class="sortable" onclick="sortTable(0)">File</th>
                    <th class="sortable" onclick="sortTable(1)">Unsafe/Total Functions</th>
                    <th class="sortable" onclick="sortTable(2)">Unsafe Statements</th>
                    <th class="sortable" onclick="sortTable(3)">Static Mut</th>
                    <th class="sortable" onclick="sortTable(4)">Unwraps</th>
                </tr>
            </thead>
            <tbody>
"#,
    );

    for (filename, stats) in &report.files {
        let file_class = if stats.is_perfect() {
            "perfect-file"
        } else {
            ""
        };
        html.push_str(&format!(
            r#"
                <tr>
                    <td class="{}">{}</td>
                    <td class="{}">{}/{}</td>
                    <td class="{}">{}</td>
                    <td class="{}">{}</td>
                    <td class="{}">{}</td>
                </tr>
"#,
            file_class,
            filename,
            get_safety_class(stats.unsafe_fns, stats.total_fns),
            stats.unsafe_fns,
            stats.total_fns,
            get_count_class(stats.unsafe_statements),
            stats.unsafe_statements,
            get_count_class(stats.static_mut_items),
            stats.static_mut_items,
            get_count_class(stats.unwraps),
            stats.unwraps
        ));
    }

    html.push_str(
        r#"
            </tbody>
        </table>
"#,
    );

    // Add baseline comparison if provided
    if let Some(baseline_file) = &args.baseline
        && let Ok(mut reader) = csv::Reader::from_path(baseline_file)
    {
        let headers: Vec<String> = reader
            .headers()
            .expect("must have headers")
            .into_iter()
            .map(|h| h.to_string())
            .collect();

        if headers == CodeStats::csv_headers() {
            let files = reader
                .records()
                .flat_map(|result| {
                    let record = result.unwrap();
                    let row: [&str; 8] = record.deserialize(None).ok()?;
                    CodeStats::from_csv_row(&row)
                })
                .collect::<std::collections::BTreeMap<String, CodeStats>>();

            let old_report = Report {
                total: files.values().cloned().sum(),
                files,
            };

            let diff = report.diff(&old_report);
            html.push_str(&format_html_diff(&diff));
        }
    }

    // JavaScript for table sorting
    html.push_str(
        r#"
    </div>
    <script>
        let sortDirections = {};

        function sortTable(column) {
            const table = document.getElementById('fileTable');
            const tbody = table.getElementsByTagName('tbody')[0];
            const rows = Array.from(tbody.getElementsByTagName('tr'));

            const direction = sortDirections[column] === 'asc' ? 'desc' : 'asc';
            sortDirections[column] = direction;

            // Clear all sort indicators
            document.querySelectorAll('th').forEach(th => {
                th.className = th.className.replace(/sort-(asc|desc)/, '');
                if (!th.className.includes('sortable')) th.className += ' sortable';
            });

            // Add sort indicator to current column
            const th = table.getElementsByTagName('th')[column];
            th.className = th.className.replace('sortable', `sortable sort-${direction}`);

            rows.sort((a, b) => {
                let aVal = a.cells[column].textContent.trim();
                let bVal = b.cells[column].textContent.trim();

                // Handle numeric columns
                if (column > 0) {
                    if (column === 1) {
                        // Unsafe/Total format
                        aVal = parseInt(aVal.split('/')[0]) || 0;
                        bVal = parseInt(bVal.split('/')[0]) || 0;
                    } else {
                        aVal = parseInt(aVal) || 0;
                        bVal = parseInt(bVal) || 0;
                    }
                }

                if (direction === 'asc') {
                    return aVal > bVal ? 1 : -1;
                } else {
                    return aVal < bVal ? 1 : -1;
                }
            });

            rows.forEach(row => tbody.appendChild(row));
        }
    </script>
</body>
</html>
"#,
    );

    html
}

fn get_safety_class(unsafe_count: isize, total_count: isize) -> &'static str {
    if total_count == 0 {
        "neutral"
    } else if unsafe_count == 0 {
        "safe"
    } else if (unsafe_count as f64 / total_count as f64) < 0.5 {
        "warning"
    } else {
        "danger"
    }
}

fn get_count_class(count: isize) -> &'static str {
    if count == 0 {
        "safe"
    } else if count < 10 {
        "warning"
    } else {
        "danger"
    }
}

fn format_html_diff(diff: &DiffReport) -> String {
    if diff.changes.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    html.push_str(
        r#"
        <div class="diff-section">
            <h2>📊 Changes from Baseline</h2>
            <div class="diff-summary">
"#,
    );

    html.push_str(&format!(
        r#"
                <div class="diff-change">
                    <strong>Summary Changes:</strong><br>
                    Unsafe functions: {} → {} ({})<br>
                    Unsafe statements: {} → {} ({})<br>
                    Static mut items: {} → {} ({})<br>
                    Unwrap calls: {} → {} ({})
                </div>
"#,
        diff.before_total.unsafe_fns,
        diff.after_total.unsafe_fns,
        format_change_delta(diff.before_total.unsafe_fns, diff.after_total.unsafe_fns),
        diff.before_total.unsafe_statements,
        diff.after_total.unsafe_statements,
        format_change_delta(
            diff.before_total.unsafe_statements,
            diff.after_total.unsafe_statements
        ),
        diff.before_total.static_mut_items,
        diff.after_total.static_mut_items,
        format_change_delta(
            diff.before_total.static_mut_items,
            diff.after_total.static_mut_items
        ),
        diff.before_total.unwraps,
        diff.after_total.unwraps,
        format_change_delta(diff.before_total.unwraps, diff.after_total.unwraps)
    ));

    for (filename, change) in &diff.changes {
        match change {
            Diff::Added(stats) => {
                html.push_str(&format!(
                    r#"
                    <div class="diff-change" style="border-left: 4px solid #27ae60;">
                        <strong>📄 {} [NEW FILE]</strong><br>
                        Unsafe functions: {}, Unsafe statements: {}, Unwraps: {}
                    </div>
"#,
                    filename, stats.unsafe_fns, stats.unsafe_statements, stats.unwraps
                ));
            }
            Diff::Removed(stats) => {
                html.push_str(&format!(
                    r#"
                    <div class="diff-change" style="border-left: 4px solid #e74c3c;">
                        <strong>🗑️ {} [REMOVED]</strong><br>
                        Had {} unsafe functions, {} unsafe statements, {} unwraps
                    </div>
"#,
                    filename, stats.unsafe_fns, stats.unsafe_statements, stats.unwraps
                ));
            }
            Diff::Changed(change) => {
                html.push_str(&format!(
                    r#"
                    <div class="diff-change" style="border-left: 4px solid #f39c12;">
                        <strong>📝 {} [MODIFIED]</strong><br>
                        Unsafe functions: {} → {} ({})<br>
                        Unsafe statements: {} → {} ({})<br>
                        Unwraps: {} → {} ({})
                    </div>
"#,
                    filename,
                    change.before.unsafe_fns,
                    change.after.unsafe_fns,
                    format_change_delta(change.before.unsafe_fns, change.after.unsafe_fns),
                    change.before.unsafe_statements,
                    change.after.unsafe_statements,
                    format_change_delta(
                        change.before.unsafe_statements,
                        change.after.unsafe_statements
                    ),
                    change.before.unwraps,
                    change.after.unwraps,
                    format_change_delta(change.before.unwraps, change.after.unwraps)
                ));
            }
        }
    }

    html.push_str(
        r#"
            </div>
        </div>
"#,
    );

    html
}
