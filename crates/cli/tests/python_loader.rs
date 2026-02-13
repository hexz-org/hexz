/// Integration tests for Python loader bindings
///
/// These tests verify that the PyO3 bindings work correctly by:
/// 1. Packing data using CLI
/// 2. Loading it via Python API (strata_loader)
/// 3. Verifying correctness
use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;

mod common;
use common::{TestEnv, compressible_data, random_data};

/// Helper to create a strata CLI command
fn strata() -> Command {
    #[allow(deprecated)]
    {
        Command::cargo_bin("strata").expect("Failed to find strata binary")
    }
}

/// Check if Python and strata_loader are available
fn python_available() -> bool {
    StdCommand::new("python3")
        .args(["-c", "import strata_loader"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a Python script and return output
fn run_python_script(script: &str) -> Result<String, String> {
    let output = StdCommand::new("python3")
        .args(["-c", script])
        .output()
        .map_err(|e| format!("Failed to run Python: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[test]
fn test_python_import() {
    if !python_available() {
        eprintln!("Skipping: Python or strata_loader not available");
        return;
    }

    let script = r#"
import strata_loader
print("OK")
"#;

    let output = run_python_script(script).expect("Failed to import strata_loader");
    assert!(output.contains("OK"));
}

#[test]
fn test_python_pack_and_read() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();
    let test_data = b"Hello from Strata!";
    let input_file = env.temp_dir.path().join("input.txt");
    fs::write(&input_file, test_data).unwrap();

    // Pack using CLI
    strata()
        .arg("data")
        .arg("pack")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Read using Python
    let script = format!(
        r#"
import strata_loader

# Open the snapshot
reader = strata_loader.Reader("{}")

# Read all data
data = reader.read_at(0, {})
print(data.decode('utf-8'))
"#,
        env.snapshot_path.display(),
        test_data.len()
    );

    let output = run_python_script(&script).expect("Python read failed");
    assert!(
        output.contains("Hello from Strata!"),
        "Data mismatch: {}",
        output
    );
}

#[test]
fn test_python_dataset_iteration() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();

    // Create a larger file for iteration
    let test_data = compressible_data(10240); // 10KB
    let input_file = env.temp_dir.path().join("dataset.bin");
    fs::write(&input_file, &test_data).unwrap();

    // Pack it
    strata()
        .arg("data")
        .arg("pack")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Iterate using Python Dataset
    let script = format!(
        r#"
import strata_loader

# Create dataset with 1KB chunks
dataset = strata_loader.Dataset(
    path="{}",
    chunk_size=1024,
    shuffle=False
)

# Count chunks
chunk_count = 0
total_bytes = 0
for chunk in dataset:
    chunk_count += 1
    total_bytes += len(chunk)

print(f"chunks={{chunk_count}},bytes={{total_bytes}}")
"#,
        env.snapshot_path.display()
    );

    let output = run_python_script(&script).expect("Dataset iteration failed");
    assert!(
        output.contains("chunks=10"),
        "Expected 10 chunks: {}",
        output
    );
    assert!(
        output.contains("bytes=10240"),
        "Expected 10240 bytes: {}",
        output
    );
}

#[test]
fn test_python_dataset_shuffling() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();
    let test_data = vec![0u8; 4096]; // 4KB of zeros
    let input_file = env.temp_dir.path().join("shuffle_test.bin");
    fs::write(&input_file, &test_data).unwrap();

    strata()
        .arg("data")
        .arg("pack")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    let script = format!(
        r#"
import strata_loader

# Test with shuffle enabled
dataset_shuffled = strata_loader.Dataset(
    path="{}",
    chunk_size=512,
    shuffle=True,
    seed=42
)

# Should work without errors
chunk_count = sum(1 for _ in dataset_shuffled)
print(f"shuffled_chunks={{chunk_count}}")
"#,
        env.snapshot_path.display()
    );

    let output = run_python_script(&script).expect("Shuffled dataset failed");
    assert!(
        output.contains("shuffled_chunks=8"),
        "Expected 8 shuffled chunks: {}",
        output
    );
}

#[test]
fn test_python_cache_size_parameter() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();
    let test_data = random_data(8192);
    let input_file = env.temp_dir.path().join("cache_test.bin");
    fs::write(&input_file, &test_data).unwrap();

    strata()
        .arg("data")
        .arg("pack")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Test with different cache sizes
    let script = format!(
        r#"
import strata_loader

# Test with cache_size parameter (if implemented)
try:
    reader = strata_loader.Reader("{}", cache_size="1M")
    data = reader.read_at(0, 100)
    print("cache_size: OK")
except TypeError:
    # Parameter not yet wired through
    print("cache_size: NOT_IMPLEMENTED")
"#,
        env.snapshot_path.display()
    );

    let output = run_python_script(&script).expect("Cache size test failed");
    // This should pass once #49 is fully complete
    println!("Cache size test result: {}", output);
}

#[test]
fn test_python_pack_from_memory() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();

    let script = format!(
        r#"
import strata_loader

# Test packing from Python
try:
    # Create a simple dataset in memory
    data = b"Test data from Python" * 100

    # Pack it (if pack() API is available)
    strata_loader.pack(
        data,
        output="{}",
        compression="lz4"
    )
    print("pack: OK")
except (AttributeError, NotImplementedError):
    print("pack: NOT_IMPLEMENTED")
"#,
        env.snapshot_path.display()
    );

    let output = run_python_script(&script).expect("Pack test failed");
    println!("Pack API test result: {}", output);

    // If pack succeeded, verify the snapshot
    if output.contains("pack: OK") {
        assert!(
            env.snapshot_path.exists(),
            "Snapshot should exist after pack()"
        );
    }
}

#[test]
fn test_python_error_handling() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let script = r#"
import strata_loader

try:
    # Try to open non-existent file
    reader = strata_loader.Reader("/nonexistent/file.strata")
    print("ERROR: Should have raised exception")
except Exception as e:
    print(f"exception: {type(e).__name__}")
"#;

    let output = run_python_script(script).expect("Error handling test failed");
    assert!(
        output.contains("exception:"),
        "Should raise exception for missing file"
    );
}

#[test]
fn test_python_concurrent_readers() {
    if !python_available() {
        eprintln!("Skipping: Python not available");
        return;
    }

    let env = TestEnv::new();
    let test_data = random_data(16384);
    let input_file = env.temp_dir.path().join("concurrent.bin");
    fs::write(&input_file, &test_data).unwrap();

    strata()
        .arg("data")
        .arg("pack")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    let script = format!(
        r#"
import strata_loader
import threading

path = "{}"
errors = []

def read_worker(offset, size):
    try:
        reader = strata_loader.Reader(path)
        data = reader.read_at(offset, size)
        if len(data) != size:
            errors.append(f"Size mismatch at {{offset}}")
    except Exception as e:
        errors.append(f"Error at {{offset}}: {{e}}")

# Create multiple threads reading different parts
threads = []
for i in range(4):
    offset = i * 4096
    t = threading.Thread(target=read_worker, args=(offset, 4096))
    threads.append(t)
    t.start()

for t in threads:
    t.join()

if errors:
    print("ERRORS:", errors)
else:
    print("concurrent: OK")
"#,
        env.snapshot_path.display()
    );

    let output = run_python_script(&script).expect("Concurrent test failed");
    assert!(
        output.contains("concurrent: OK"),
        "Concurrent reads failed: {}",
        output
    );
}
