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

// this should not be detected, it returns -1
pub unsafe fn cmd_find_from_mouse(
    fs: *mut cmd_find_state,
    m: *mut mouse_event,
    flags: cmd_find_flags,
) -> i32 {
    unsafe {
        cmd_find_clear_state(fs, flags);

        if !(*m).valid {
            return -1;
        }

        (*fs).wp = transmute_ptr(cmd_mouse_pane(m, &raw mut (*fs).s, &raw mut (*fs).wl));
        if (*fs).wp.is_null() {
            cmd_find_clear_state(fs, flags);
            return -1;
        }
        (*fs).w = (*(*fs).wl).window;

        cmd_find_log_state(c!("cmd_find_from_mouse"), fs);
    }
    0
}



// Helper functions (don't matter for the analysis)
fn some_condition() -> bool { true }
fn get_state() -> Option<i32> { Some(1) }
fn error_condition() -> bool { false }
fn calculate() -> i32 { 42 }
fn ready() -> bool { true }
