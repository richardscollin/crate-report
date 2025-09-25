// Test file for bool candidates analysis

// This should be detected - only returns 0 or 1
fn is_valid() -> i32 {
    if some_condition() {
        return 1;
    }
    0
}

// This should be detected - simple 0/1 returns
fn check_status() -> i32 {
    match get_state() {
        Some(x) if x > 0 => 1,
        _ => 0,
    }
}

// This should NOT be detected - returns other values
fn get_code() -> i32 {
    if error_condition() {
        return -1;
    }
    0
}

// This should NOT be detected - doesn't return i32
fn is_enabled() -> bool {
    true
}

// This should NOT be detected - returns variable values
fn compute_value() -> i32 {
    let x = calculate();
    return x;
}

// This should be detected - only literal 0 and 1
fn simple_flag() -> i32 {
    if ready() {
        1
    } else {
        0
    }
}

// Helper functions (don't matter for the analysis)
fn some_condition() -> bool { true }
fn get_state() -> Option<i32> { Some(1) }
fn error_condition() -> bool { false }
fn calculate() -> i32 { 42 }
fn ready() -> bool { true }