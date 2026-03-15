//! Progress reporting for long-running pack operations.
//!
//! Provides a consistent progress bar interface using indicatif.

use indicatif::{ProgressBar, ProgressStyle};

/// Progress tracker for packing operations.
pub struct PackProgress {
    bar: ProgressBar,
}

impl std::fmt::Debug for PackProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackProgress").finish_non_exhaustive()
    }
}

impl PackProgress {
    /// Create a new progress bar
    ///
    /// # Arguments
    ///
    /// * `total_bytes` - Total bytes to process
    /// * `message` - Initial message to display
    pub fn new(total_bytes: u64, message: &str) -> Self {
        let bar = ProgressBar::new(total_bytes);
        if let Ok(style) = ProgressStyle::default_bar()
            .template("{msg} {bar:40.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        {
            bar.set_style(style.progress_chars("█▓▒░ "));
        }
        bar.set_message(message.to_string());

        Self { bar }
    }

    /// Increment progress by delta bytes
    pub fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }

    /// Set current position directly
    pub fn set_position(&self, pos: u64) {
        self.bar.set_position(pos);
    }

    /// Finish with success message
    pub fn finish(&self) {
        self.bar.finish_with_message("Packing complete");
    }

    /// Finish with custom message
    pub fn finish_with_message(&self, msg: &str) {
        self.bar.finish_with_message(msg.to_string());
    }

    /// Update the message
    pub fn set_message(&self, msg: &str) {
        self.bar.set_message(msg.to_string());
    }
}

impl Drop for PackProgress {
    fn drop(&mut self) {
        if !self.bar.is_finished() {
            self.bar.abandon();
        }
    }
}
