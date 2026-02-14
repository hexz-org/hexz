//! Centralized progress bar and spinner utilities.
//!
//! This module provides consistent progress bar creation and styling across all
//! CLI commands, ensuring uniform user feedback during long-running operations.
//!
//! # Progress Bar Styling
//!
//! All progress bars use a standardized format:
//! ```text
//! [00:01:23] =========>------------------------------ 234MB/1GB (00:02:15)
//!  ^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^     ^^^^^^^^^^^  ^^^^^^^^
//!  elapsed   visual bar (40 chars)                    bytes        ETA
//! ```
//!
//! # Spinner Styling
//!
//! Spinners are used for indeterminate operations (e.g., waiting for network):
//! ```text
//! ⠋ Connecting to remote storage...
//! ```
//!
//! # Usage
//!
//! ```no_run
//! use hexz_cli::ui::progress::{create_progress_bar, create_spinner};
//!
//! // Determinate operation
//! let pb = create_progress_bar(1024 * 1024 * 100); // 100 MB
//! for _ in 0..100 {
//!     pb.inc(1024 * 1024); // 1 MB per iteration
//! }
//! pb.finish_with_message("Complete");
//!
//! // Indeterminate operation
//! let sp = create_spinner("Processing...");
//! std::thread::sleep(std::time::Duration::from_millis(100));
//! sp.finish_with_message("Done");
//! ```

use indicatif::{ProgressBar, ProgressStyle};

/// Creates a standardized progress bar for determinate operations.
///
/// # Arguments
///
/// * `total` - Total number of bytes or units to process
///
/// # Returns
///
/// A configured [`ProgressBar`] with:
/// - 40-character visual bar
/// - Elapsed time display
/// - Bytes progress (current/total)
/// - Estimated time to completion (ETA)
///
/// # Display Format
///
/// ```text
/// [00:01:23] =========>------------------------------ 234MB/1GB (00:02:15)
/// ```
///
/// # Example
///
/// ```no_run
/// # use hexz_cli::ui::progress::create_progress_bar;
/// let file_size = 1024 * 1024 * 100; // 100 MB
/// let pb = create_progress_bar(file_size);
///
/// for _ in 0..100 {
///     pb.inc(1024 * 1024);
/// }
/// pb.finish_with_message("Download complete");
/// ```
pub fn create_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40} {bytes}/{total_bytes} ({eta})")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=>-"),
    );
    pb
}

/// Creates a spinner for indeterminate operations.
///
/// Spinners are used when the total duration or progress cannot be determined,
/// such as waiting for network responses or performing iterative searches.
///
/// # Arguments
///
/// * `message` - Initial status message to display next to the spinner
///
/// # Returns
///
/// A configured [`ProgressBar`] in spinner mode with a green spinner animation.
///
/// # Display Format
///
/// ```text
/// ⠋ Connecting to remote storage...
/// ⠙ Connecting to remote storage...
/// ⠹ Connecting to remote storage...
/// ```
///
/// # Example
///
/// ```no_run
/// # use hexz_cli::ui::progress::create_spinner;
/// let sp = create_spinner("Initializing...");
///
/// std::thread::sleep(std::time::Duration::from_millis(100));
/// sp.set_message("Connecting...");
///
/// std::thread::sleep(std::time::Duration::from_millis(100));
/// sp.finish_with_message("Ready");
/// ```
///
/// # Notes
///
/// - The spinner auto-ticks to create the animation effect
/// - Update the message with [`set_message`](ProgressBar::set_message)
/// - Call [`finish`](ProgressBar::finish) or [`finish_with_message`](ProgressBar::finish_with_message) when done
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(message.to_string());
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_progress_bar_basic() {
        let pb = create_progress_bar(1000);
        assert_eq!(pb.length(), Some(1000));
        assert_eq!(pb.position(), 0);
    }

    #[test]
    fn test_create_progress_bar_zero_total() {
        let pb = create_progress_bar(0);
        assert_eq!(pb.length(), Some(0));
    }

    #[test]
    fn test_create_progress_bar_large_total() {
        let total = 10 * 1024 * 1024 * 1024u64; // 10 GB
        let pb = create_progress_bar(total);
        assert_eq!(pb.length(), Some(total));
    }

    #[test]
    fn test_progress_bar_increment() {
        let pb = create_progress_bar(100);
        pb.inc(50);
        assert_eq!(pb.position(), 50);
        pb.inc(50);
        assert_eq!(pb.position(), 100);
    }

    #[test]
    fn test_progress_bar_finish() {
        let pb = create_progress_bar(100);
        pb.inc(100);
        pb.finish_with_message("done");
        assert!(pb.is_finished());
    }

    #[test]
    fn test_create_spinner_basic() {
        let sp = create_spinner("Loading...");
        assert!(!sp.is_finished());
    }

    #[test]
    fn test_spinner_finish() {
        let sp = create_spinner("Working...");
        sp.finish_with_message("Complete");
        assert!(sp.is_finished());
    }

    #[test]
    fn test_spinner_update_message() {
        let sp = create_spinner("Step 1");
        sp.set_message("Step 2");
        sp.finish();
        assert!(sp.is_finished());
    }
}
