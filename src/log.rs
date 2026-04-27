use std::sync::atomic::{AtomicI8, Ordering};

/// Verbosity levels for the logger.
const LEVEL_ERROR: i8 = -2;
const LEVEL_WARN: i8 = -1;
const LEVEL_INFO: i8 = 0;
const LEVEL_DEBUG: i8 = 1;

static LEVEL: AtomicI8 = AtomicI8::new(LEVEL_INFO);

/// Initialize the logger with the given verbosity offset from default (0).
///
/// - `0` → info (default)
/// - `> 0` → debug / verbose
/// - `< 0` → quiet (suppresses info; only warnings at -1, only errors at -2)
pub fn init(verbosity: i8) {
    // Clamp to [-2, 1]
    let level = verbosity.clamp(LEVEL_ERROR, LEVEL_DEBUG);
    LEVEL.store(level, Ordering::Relaxed);
}

/// Log a debug message (only shown with `--verbose`).
pub fn debug(msg: &str) {
    if LEVEL.load(Ordering::Relaxed) >= LEVEL_DEBUG {
        eprintln!("[DEBUG] {msg}");
    }
}

/// Log an informational message (shown by default, suppressed with `--quiet`).
pub fn info(msg: &str) {
    if LEVEL.load(Ordering::Relaxed) >= LEVEL_INFO {
        eprintln!("{msg}");
    }
}

/// Log a warning message (shown unless `-qq`).
pub fn warn(msg: &str) {
    if LEVEL.load(Ordering::Relaxed) >= LEVEL_WARN {
        eprintln!("WARNING: {msg}");
    }
}

/// Log an error message (always shown).
pub fn error(msg: &str) {
    eprintln!("error: {msg}");
}

/// Format args into a string for logging (avoids allocating in non-verbose mode).
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::log::debug(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::log::info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::log::warn(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::log::error(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// All log tests touch the global LEVEL atomic, so they must run
    /// serialized — `cargo test` runs tests in parallel by default.
    static GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn test_level_clamping() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        init(5); // should clamp to 1 (debug)
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_DEBUG);

        init(-10); // should clamp to -2 (error only)
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_ERROR);

        init(0); // reset to default
    }

    #[test]
    fn test_init_default() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        init(0);
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_INFO);
    }

    #[test]
    fn test_init_quiet() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        init(-1);
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_WARN);

        init(-2);
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_ERROR);

        init(0); // reset
    }

    #[test]
    fn test_init_verbose() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        init(1);
        assert_eq!(LEVEL.load(Ordering::Relaxed), LEVEL_DEBUG);

        init(0); // reset
    }
}
