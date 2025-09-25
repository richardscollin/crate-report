use std::{
    fs,
    path::Path,
};

use syn::{
    Expr,
    ItemFn,
    ReturnType,
    Stmt,
    visit::Visit,
};
use walkdir::WalkDir;

#[derive(Clone, Default, Debug)]
pub struct FileStats {
    pub filename: String,
    pub stats: CodeStats,
}

#[derive(Clone, Default, Debug)]
pub struct BoolCandidate {
    pub fn_name: String,
    pub line_number: usize,
}

#[derive(Clone, Default, Debug)]
pub struct CodeStats {
    pub candidates: Vec<BoolCandidate>,
}

pub struct CodeAnalyzer<'a> {
    stats: &'a mut CodeStats,
}

/// Check if a type is i32
fn is_i32_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                segment.ident == "i32"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if an expression is a valid nested expression (if/match) that only returns 0 or 1
fn is_valid_nested_expression(expr: &Expr) -> bool {
    match expr {
        Expr::If(_) | Expr::Match(_) | Expr::Block(_) | Expr::Unsafe(_) => check_expr_returns_only_zero_or_one(expr),
        _ => false,
    }
}

/// Check if an expression is a literal 0 or 1
fn is_zero_or_one_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            syn::Lit::Int(lit_int) => {
                let value = lit_int.base10_parse::<i32>().unwrap_or(-999);
                value == 0 || value == 1
            }
            _ => false,
        },
        Expr::Unary(unary_expr) => {
            // Handle negative literals like -1
            if let syn::UnOp::Neg(_) = unary_expr.op {
                if let Expr::Lit(expr_lit) = &*unary_expr.expr {
                    if let syn::Lit::Int(lit_int) = &expr_lit.lit {
                        let value = lit_int.base10_parse::<i32>().unwrap_or(999);
                        let negative_value = -(value as i32);
                        return negative_value == 0 || negative_value == 1;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Check if all return statements and final expression in a block are 0 or 1 literals
fn check_block_returns_only_zero_or_one(block: &syn::Block) -> bool {
    let mut has_returns = false;

    // Check all statements for return statements
    for stmt in &block.stmts {
        match stmt {
            Stmt::Expr(expr, None) => {
                // Expression statement without semicolon (potentially implicit return)
                if let Expr::Return(return_expr) = expr {
                    has_returns = true;
                    if let Some(return_value) = &return_expr.expr {
                        if !is_zero_or_one_literal(return_value) {
                            return false;
                        }
                    } else {
                        // Return with no value (unit return)
                        return false;
                    }
                }
                // Check nested blocks, if statements, etc.
                if !check_expr_returns_only_zero_or_one(expr) {
                    return false;
                }
            }
            Stmt::Expr(expr, Some(_)) => {
                // Expression statement with semicolon
                if let Expr::Return(return_expr) = expr {
                    has_returns = true;
                    if let Some(return_value) = &return_expr.expr {
                        if !is_zero_or_one_literal(return_value) {
                            return false;
                        }
                    } else {
                        // Return with no value (unit return)
                        return false;
                    }
                }
                // Check nested blocks, if statements, etc.
                if !check_expr_returns_only_zero_or_one(expr) {
                    return false;
                }
            }
            _ => {}
        }
    }

    // Check the final expression if it exists (implicit return)
    if let Some(final_stmt) = block.stmts.last()
        && let Stmt::Expr(expr, None) = final_stmt
    {
        has_returns = true;
        // For the final expression, check if it's a 0/1 literal OR a valid nested expression
        if !is_zero_or_one_literal(expr) && !is_valid_nested_expression(expr) {
            return false;
        }
    }

    has_returns
}

/// Recursively check expressions for return statements
fn check_expr_returns_only_zero_or_one(expr: &Expr) -> bool {
    match expr {
        Expr::Return(return_expr) => {
            if let Some(return_value) = &return_expr.expr {
                is_zero_or_one_literal(return_value)
            } else {
                false // Unit return
            }
        }
        Expr::Block(block_expr) => check_block_returns_only_zero_or_one(&block_expr.block),
        Expr::Unsafe(unsafe_expr) => check_block_returns_only_zero_or_one(&unsafe_expr.block),
        Expr::If(if_expr) => {
            // Check the then branch
            if !check_block_returns_only_zero_or_one(&if_expr.then_branch) {
                return false;
            }

            // Check the else branch if it exists
            if let Some((_, else_branch)) = &if_expr.else_branch
                && !check_expr_returns_only_zero_or_one(else_branch)
            {
                return false;
            }

            true
        }
        Expr::Match(match_expr) => {
            // Check all match arms
            for arm in &match_expr.arms {
                if let Expr::Block(block_expr) = &*arm.body {
                    if !check_block_returns_only_zero_or_one(&block_expr.block) {
                        return false;
                    }
                } else if !is_zero_or_one_literal(&arm.body)
                    && !check_expr_returns_only_zero_or_one(&arm.body)
                {
                    return false;
                }
            }
            true
        }
        // For other expressions, we don't find return statements, so they pass
        _ => true,
    }
}

impl<'a, 'ast> Visit<'ast> for CodeAnalyzer<'a> {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        use syn::spanned::Spanned;

        // Check if function returns i32
        if let ReturnType::Type(_, return_type) = &i.sig.output
            && is_i32_type(return_type)
        {
            // Analyze the function body to see if it only returns 0 or 1
            if check_block_returns_only_zero_or_one(&i.block) {
                let candidate = BoolCandidate {
                    fn_name: i.sig.ident.to_string(),
                    line_number: i.span().start().line,
                };
                self.stats.candidates.push(candidate);
            }
        }

        syn::visit::visit_item_fn(self, i);
    }
}

fn analyze_file(path: &Path) -> Option<FileStats> {
    let content = fs::read_to_string(path).ok()?;
    let syntax = syn::parse_file(&content).ok()?;

    let mut stats = CodeStats::default();
    let mut visitor = CodeAnalyzer { stats: &mut stats };
    visitor.visit_file(&syntax);

    Some(FileStats {
        filename: path.display().to_string(),
        stats,
    })
}

/// Find good candidates for functions to convert from returning i32 to bool
/// The heuristic is if the function returns i32 and all return statements
/// and the final expression return literal 0 or 1 values
pub fn find_candidates(root: impl AsRef<Path>) -> Vec<FileStats> {
    let root = root.as_ref();
    let mut file_reports = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map(|s| s != "target")
                .unwrap_or(true)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "rs").unwrap_or(false))
    {
        let path = entry.path();
        if let Some(file_stats) = analyze_file(path) {
            file_reports.push(file_stats);
        }
    }

    // Strip common root prefix and find max filename length for alignment
    let mut max_filename_len = 0;
    for file_report in &mut file_reports {
        if let Ok(relative_path) = Path::new(&file_report.filename).strip_prefix(root) {
            file_report.filename = relative_path.display().to_string();
        }
        max_filename_len = max_filename_len.max(file_report.filename.len());
    }

    file_reports.sort_by(|a, b| a.filename.cmp(&b.filename));
    file_reports.retain(|r| !r.stats.candidates.is_empty());
    file_reports
}
