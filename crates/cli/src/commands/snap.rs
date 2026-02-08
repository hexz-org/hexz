//! Live snapshot creation via QMP.

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

use crate::commands::commit;

const QMP_POLL_SLEEP: Duration = Duration::from_millis(500);
const QMP_BUFFER_SIZE: usize = 4096;
const DEFAULT_COMMIT_BLOCK_SIZE: u32 = 65536;

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
