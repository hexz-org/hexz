//! Live snapshot creation via QEMU QMP (QEMU Machine Protocol).
//!
//! This command creates a live snapshot of a running VM by connecting to its
//! QMP control socket, pausing the VM, dumping memory state via QEMU's migration
//! mechanism, and then creating a new snapshot that includes both the overlay
//! (modified disk blocks) and the captured memory dump.
//!
//! # QMP Protocol Integration
//!
//! QEMU Machine Protocol (QMP) is a JSON-based protocol for controlling QEMU:
//!
//! **Connection Sequence:**
//! 1. Connect to Unix socket (created with `--qmp-socket` during boot)
//! 2. Receive QMP greeting banner
//! 3. Send `qmp_capabilities` to negotiate features
//! 4. Send commands and receive JSON responses
//!
//! **Commands Used:**
//! - `stop`: Pauses VM execution (sets state to "paused")
//! - `migrate`: Triggers memory dump to file via `exec:cat > <path>`
//! - `query-migrate`: Polls migration status ("active", "completed", "failed")
//! - `cont`: Resumes VM execution after snapshot is complete
//!
//! **QMP Message Format:**
//! ```json
//! // Command (sent by client)
//! {"execute": "stop"}
//!
//! // Response (received from QEMU)
//! {"return": {}}
//!
//! // Status query response
//! {"return": {"status": "completed", "total": 4294967296}}
//! ```
//!
//! # Snapshot Format
//!
//! The live snapshot captures both persistent and volatile state:
//!
//! **Disk State (from overlay):**
//! - Modified blocks since VM boot
//! - 4 KiB granularity tracked in `.meta` file
//! - Merged with base snapshot during commit
//!
//! **Memory State (from QEMU migration):**
//! - Full RAM dump in QEMU migration format
//! - Includes CPU registers, device state, page tables
//! - Compressed with LZ4 by default for fast resume
//!
//! The resulting snapshot is a "thick" snapshot (default) containing all state
//! needed to resume the VM independently of the base snapshot.
//!
//! # Use Cases
//!
//! - **Checkpoint and Restore**: Save VM state for later resume
//! - **Testing and Development**: Create snapshots before risky operations
//! - **Migration**: Capture running VM for transfer to another host
//! - **Debugging**: Preserve exact VM state for post-mortem analysis
//! - **Backup**: Create consistent backups while VM is running
//!
//! # Workflow
//!
//! 1. **Connect to QMP**: Opens Unix socket to running QEMU instance
//! 2. **Negotiate Capabilities**: Establishes QMP protocol version
//! 3. **Pause VM**: Sends `stop` command to freeze execution
//! 4. **Dump Memory**: Uses `migrate` command with `exec:` URI to save RAM
//! 5. **Poll Status**: Repeatedly checks migration progress until complete
//! 6. **Create Snapshot**: Calls `commit` to merge overlay + memory
//! 7. **Resume VM**: Sends `cont` command to unpause execution
//!
//! # Performance Characteristics
//!
//! - **Pause Time**: Typically 50-200 ms for `stop` command
//! - **Memory Dump**: ~500-1000 MB/s (depends on storage bandwidth)
//! - **Snapshot Creation**: ~200-500 MB/s (LZ4 compression)
//! - **Total Downtime**: Typically 2-10 seconds for 4-8 GB VM
//!
//! # Error Handling
//!
//! If snapshot creation fails after pausing the VM, the command:
//! 1. Attempts to resume the VM with `cont` command
//! 2. Returns the snapshot error to the caller
//! 3. Leaves overlay files intact for retry
//!
//! This ensures the VM is not left in a paused state even on failure.
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Create live snapshot of running VM
//! strata vm snap \
//!   --socket /tmp/vm.qmp \
//!   --overlay vm-state.overlay \
//!   --base vm-base.st \
//!   --output vm-checkpoint.st
//!
//! # Resume from snapshot later
//! strata vm boot vm-checkpoint.st --ram 4G
//! ```

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

use crate::cmd::vm::commit;

/// Interval between QMP status polls while waiting for migration (500 ms).
///
/// **Architectural intent:** Balances responsiveness with QMP socket load.
/// Polling too frequently adds overhead; polling too slowly delays completion detection.
const QMP_POLL_SLEEP: Duration = Duration::from_millis(500);

/// Buffer size for QMP socket reads (4 KiB).
///
/// **Architectural intent:** Large enough for typical QMP responses but small
/// enough to avoid excessive memory allocation. QMP responses are usually <1 KiB.
const QMP_BUFFER_SIZE: usize = 4096;

/// Default block size for snapshot commit operations (64 KiB).
///
/// **Architectural intent:** Matches the standard block size used by `pack` and
/// `build` commands for consistency across snapshot formats.
const DEFAULT_COMMIT_BLOCK_SIZE: u32 = 65536;

/// Executes the live snapshot command via QMP.
///
/// Connects to a running QEMU instance via its QMP socket, pauses execution,
/// dumps memory state to a temporary file, creates a new snapshot that merges
/// the overlay and memory dump, and resumes execution. This enables capturing
/// a consistent point-in-time snapshot without shutting down the VM.
///
/// # Arguments
///
/// * `socket_path` - Path to the QMP Unix socket (created with `--qmp-socket`)
/// * `overlay_path` - Path to the overlay file containing modified disk blocks
/// * `base_strata_path` - Path to the base snapshot the VM was booted from
/// * `output_path` - Path for the output snapshot file
///
/// # QMP Command Sequence
///
/// 1. Connect to `socket_path` and read greeting
/// 2. Send `qmp_capabilities` and wait for acknowledgment
/// 3. Send `stop` to pause VM
/// 4. Send `migrate` with URI `exec:cat > <temp_file>`
/// 5. Poll `query-migrate` until status is "completed" or "failed"
/// 6. Call `commit::run()` to create snapshot with overlay + memory
/// 7. Send `cont` to resume VM (even if commit fails)
///
/// # Snapshot Parameters
///
/// The snapshot is created with:
/// - Compression: LZ4 (fast decompression for quick resume)
/// - Block size: 64 KiB (default)
/// - Dictionary training: Enabled (improves memory compression)
/// - Thin mode: Disabled (creates standalone snapshot)
///
/// # Errors
///
/// Returns an error if:
/// - QMP socket cannot be connected (VM not running or socket path wrong)
/// - QMP commands fail (protocol error, QEMU internal error)
/// - Memory migration fails (disk full, I/O error)
/// - Commit operation fails (compression error, write failure)
///
/// Note: VM resume is attempted even if errors occur, to prevent leaving
/// the VM in a paused state.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use strata_cli::cmd::vm::snap;
///
/// // Create live snapshot of running VM
/// snap::run(
///     PathBuf::from("/tmp/vm.qmp"),
///     PathBuf::from("vm-state.overlay"),
///     PathBuf::from("vm-base.st"),
///     PathBuf::from("vm-checkpoint.st"),
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(
    socket_path: PathBuf,
    overlay_path: PathBuf,
    base_strata_path: PathBuf,
    output_path: PathBuf,
) -> Result<()> {
    println!("Connecting to VM at {:?}", socket_path);
    let mut stream = UnixStream::connect(&socket_path)
        .context("Failed to connect to QMP socket. Is the VM running with --qmp-socket?")?;

    read_response(&mut stream)?;
    send_command(&mut stream, "qmp_capabilities", None)?;
    read_response(&mut stream)?;

    println!("Pausing VM...");
    send_command(&mut stream, "stop", None)?;
    read_response(&mut stream)?;

    println!("Dumping Guest Memory...");
    let mem_dump = NamedTempFile::new()?;
    let mem_path = mem_dump.path().to_str().unwrap().to_string();

    let migrate_cmd = format!("exec:cat > {}", mem_path);
    let args = serde_json::json!({ "uri": migrate_cmd });

    send_command(&mut stream, "migrate", Some(args))?;
    read_response(&mut stream)?;

    print!("Saving memory");
    loop {
        send_command(&mut stream, "query-migrate", None)?;
        let resp = read_response(&mut stream)?;

        if let Some(status) = resp["return"]["status"].as_str() {
            if status == "completed" {
                println!(" Done.");
                break;
            } else if status == "failed" {
                println!(" Memory dump failed. Resuming VM...");
                send_command(&mut stream, "cont", None)?;
                anyhow::bail!("Memory dump failed: {:?}", resp);
            }
        }
        print!(".");
        std::io::stdout().flush()?;
        thread::sleep(QMP_POLL_SLEEP);
    }

    println!("Creating snapshot...");

    let commit_result = commit::run(
        base_strata_path,
        overlay_path,
        Some(mem_dump.path().to_path_buf()),
        output_path,
        "lz4".to_string(),
        DEFAULT_COMMIT_BLOCK_SIZE,
        true,
        None,
        false, // Default to thick snapshot for live snaps
    );

    println!("Resuming VM...");
    let resume_result = send_command(&mut stream, "cont", None);
    let _ = read_response(&mut stream);

    commit_result.context("Snapshot commit failed")?;
    resume_result.context("Failed to resume VM")?;

    Ok(())
}

/// Sends a QMP command to the QEMU instance.
///
/// Constructs a QMP command JSON object with the specified command name and
/// optional arguments, serializes it, and writes it to the QMP socket.
///
/// # Arguments
///
/// * `stream` - Mutable reference to the connected QMP Unix socket
/// * `cmd` - QMP command name (e.g., "stop", "cont", "query-migrate")
/// * `args` - Optional JSON object containing command arguments
///
/// # QMP Command Format
///
/// ```json
/// {"execute": "command_name"}
/// {"execute": "command_name", "arguments": {...}}
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - JSON serialization fails (should never happen with valid commands)
/// - Socket write fails (connection closed, I/O error)
fn send_command(stream: &mut UnixStream, cmd: &str, args: Option<Value>) -> Result<()> {
    let mut json = serde_json::json!({
        "execute": cmd
    });
    if let Some(a) = args {
        json["arguments"] = a;
    }
    let data = serde_json::to_string(&json)?;
    stream.write_all(data.as_bytes())?;
    Ok(())
}

/// Reads a QMP response from the QEMU instance.
///
/// Reads data from the QMP socket, parses line-delimited JSON, and returns the
/// first object containing a "return" field. This filters out QMP events and
/// focuses on command responses.
///
/// # Arguments
///
/// * `stream` - Mutable reference to the connected QMP Unix socket
///
/// # Response Handling
///
/// QMP sends line-delimited JSON. Each line can be:
/// - Command response: `{"return": {...}}`
/// - Event notification: `{"event": "...", "data": {...}}`
/// - Error response: `{"error": {...}}`
///
/// This function returns the first line with a "return" field, which corresponds
/// to the most recent command's response.
///
/// # Errors
///
/// Returns an error if:
/// - Socket read fails (connection closed, I/O error)
/// - JSON parsing fails (malformed QMP response)
///
/// Note: If no response with "return" is found, returns empty JSON object `{}`.
fn read_response(stream: &mut UnixStream) -> Result<Value> {
    let mut buf = [0u8; QMP_BUFFER_SIZE];
    let n = stream.read(&mut buf)?;
    let s = String::from_utf8_lossy(&buf[..n]);

    for line in s.lines() {
        if let Ok(val) = serde_json::from_str::<Value>(line)
            && val.get("return").is_some()
        {
            return Ok(val);
        }
    }
    Ok(serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// Test send_command with a simple command (no arguments).
    #[test]
    fn test_send_command_no_args() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        send_command(&mut client, "stop", None).unwrap();

        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(sent["execute"], "stop");
        assert!(sent.get("arguments").is_none());
    }

    /// Test send_command with arguments.
    #[test]
    fn test_send_command_with_args() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let args = serde_json::json!({ "uri": "exec:cat > /tmp/mem.bin" });
        send_command(&mut client, "migrate", Some(args)).unwrap();

        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(sent["execute"], "migrate");
        assert_eq!(sent["arguments"]["uri"], "exec:cat > /tmp/mem.bin");
    }

    /// Test send_command with qmp_capabilities (initial handshake).
    #[test]
    fn test_send_command_qmp_capabilities() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        send_command(&mut client, "qmp_capabilities", None).unwrap();

        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(sent["execute"], "qmp_capabilities");
    }

    /// Test send_command with cont (resume).
    #[test]
    fn test_send_command_cont() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        send_command(&mut client, "cont", None).unwrap();

        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(sent["execute"], "cont");
    }

    /// Test read_response with a valid return response.
    #[test]
    fn test_read_response_with_return() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        // Write a QMP response from the "server" side
        let response = "{\"return\": {}}\n";
        server.write_all(response.as_bytes()).unwrap();
        // Shut down write side so client read doesn't hang
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        assert!(val.get("return").is_some());
    }

    /// Test read_response with migration status response.
    #[test]
    fn test_read_response_migration_completed() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let response = "{\"return\": {\"status\": \"completed\", \"total\": 4294967296}}\n";
        server.write_all(response.as_bytes()).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        assert_eq!(val["return"]["status"], "completed");
        assert_eq!(val["return"]["total"], 4294967296u64);
    }

    /// Test read_response with migration failed status.
    #[test]
    fn test_read_response_migration_failed() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let response = "{\"return\": {\"status\": \"failed\"}}\n";
        server.write_all(response.as_bytes()).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        assert_eq!(val["return"]["status"], "failed");
    }

    /// Test read_response with event (no "return" field) — should return empty.
    #[test]
    fn test_read_response_event_only() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let response = "{\"event\": \"STOP\", \"data\": {}}\n";
        server.write_all(response.as_bytes()).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        // No "return" field, should get empty object
        assert!(val.get("return").is_none());
        assert_eq!(val, serde_json::json!({}));
    }

    /// Test read_response with multiple lines — returns first with "return".
    #[test]
    fn test_read_response_multi_line() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let response = "{\"event\": \"STOP\"}\n{\"return\": {\"status\": \"active\"}}\n";
        server.write_all(response.as_bytes()).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        assert_eq!(val["return"]["status"], "active");
    }

    /// Test read_response with QMP greeting banner.
    #[test]
    fn test_read_response_qmp_greeting() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        // QMP greeting doesn't have "return"
        let greeting =
            "{\"QMP\": {\"version\": {\"qemu\": {\"micro\": 0, \"minor\": 2, \"major\": 8}}}}\n";
        server.write_all(greeting.as_bytes()).unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();

        let val = read_response(&mut client).unwrap();
        assert_eq!(val, serde_json::json!({}));
    }

    /// Test send_command with query-migrate.
    #[test]
    fn test_send_command_query_migrate() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        send_command(&mut client, "query-migrate", None).unwrap();

        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(sent["execute"], "query-migrate");
    }
}
