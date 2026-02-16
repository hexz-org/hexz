//! Pack Memory Macro-Benchmark.
//!
//! This benchmark measures process memory (RSS) during pack operations at
//! increasing dataset sizes (50 MB, 200 MB, 500 MB). It records both RSS
//! delta (before/after pack) and the theoretical dedup map memory estimate
//! to help validate and tune dedup table memory usage (see issue #116).
//!
//! Memory numbers are printed via `eprintln!` (same pattern as `dedup_efficiency`).

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Platform-specific RSS helpers
// ---------------------------------------------------------------------------

/// Returns the current resident set size (RSS) of this process in bytes.
///
/// - **Windows**: Uses `GetProcessMemoryInfo` (K32/psapi).
/// - **Unix/Linux**: Reads `/proc/self/statm` (Linux) or falls back to
///   `getrusage` (other Unix).
/// - **Unsupported**: Returns 0.
fn get_current_rss() -> usize {
    #[cfg(target_os = "windows")]
    {
        get_current_rss_windows()
    }
    #[cfg(target_os = "linux")]
    {
        get_current_rss_linux()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        get_current_rss_unix()
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        0
    }
}

#[cfg(target_os = "windows")]
fn get_current_rss_windows() -> usize {
    use std::mem::{self, MaybeUninit};

    // PROCESS_MEMORY_COUNTERS_EX layout (Windows SDK)
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
        PrivateUsage: usize,
    }

    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            pmc: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }

    unsafe {
        let mut pmc = MaybeUninit::<ProcessMemoryCountersEx>::zeroed().assume_init();
        pmc.cb = mem::size_of::<ProcessMemoryCountersEx>() as u32;
        let handle = GetCurrentProcess();
        if K32GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
            pmc.WorkingSetSize
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
fn get_current_rss_linux() -> usize {
    // /proc/self/statm fields are in pages; multiply by page size.
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        if let Some(rss_pages) = statm.split_whitespace().nth(1) {
            if let Ok(pages) = rss_pages.parse::<usize>() {
                let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
                return pages * page_size;
            }
        }
    }
    0
}

#[cfg(all(unix, not(target_os = "linux")))]
fn get_current_rss_unix() -> usize {
    // macOS/BSD: getrusage reports ru_maxrss in bytes (macOS) or KB (other BSD).
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            #[cfg(target_os = "macos")]
            {
                usage.ru_maxrss as usize // bytes on macOS
            }
            #[cfg(not(target_os = "macos"))]
            {
                (usage.ru_maxrss as usize) * 1024 // KB on other BSDs
            }
        } else {
            0
        }
    }
}

/// Returns the peak RSS of this process in bytes.
///
/// Uses `getrusage` on Unix or `GetProcessMemoryInfo` on Windows.
fn get_peak_rss() -> usize {
    #[cfg(target_os = "windows")]
    {
        get_peak_rss_windows()
    }
    #[cfg(unix)]
    {
        get_peak_rss_unix()
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        0
    }
}

#[cfg(target_os = "windows")]
fn get_peak_rss_windows() -> usize {
    use std::mem::{self, MaybeUninit};

    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
        PrivateUsage: usize,
    }

    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            pmc: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }

    unsafe {
        let mut pmc = MaybeUninit::<ProcessMemoryCountersEx>::zeroed().assume_init();
        pmc.cb = mem::size_of::<ProcessMemoryCountersEx>() as u32;
        let handle = GetCurrentProcess();
        if K32GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
            pmc.PeakWorkingSetSize
        } else {
            0
        }
    }
}

#[cfg(unix)]
fn get_peak_rss_unix() -> usize {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            #[cfg(target_os = "macos")]
            {
                usage.ru_maxrss as usize // bytes on macOS
            }
            #[cfg(not(target_os = "macos"))]
            {
                (usage.ru_maxrss as usize) * 1024 // KB on Linux/BSD
            }
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Test data generation
// ---------------------------------------------------------------------------

/// Creates a test file with a specified duplication percentage.
///
/// Generates blocks of random data, then repeats earlier blocks to hit the
/// desired duplication ratio. This mirrors real-world dedup workloads where
/// a portion of blocks are duplicates.
fn create_test_file_with_duplication(
    size: usize,
    duplication_pct: f64,
    temp_dir: &TempDir,
) -> PathBuf {
    let file_path = temp_dir.path().join("test_data.bin");
    let mut file = File::create(&file_path).unwrap();

    let block_size = 65536; // 64 KB — matches default pack block size
    let num_blocks = size / block_size;
    let unique_blocks = ((num_blocks as f64) * (1.0 - duplication_pct)).max(1.0) as usize;

    let mut rng = StdRng::seed_from_u64(42);

    // Pre-generate unique blocks
    let mut unique_data: Vec<Vec<u8>> = Vec::with_capacity(unique_blocks);
    for _ in 0..unique_blocks {
        let mut block = vec![0u8; block_size];
        for byte in &mut block {
            *byte = rng.r#gen::<u8>();
        }
        unique_data.push(block);
    }

    // Write blocks, repeating for duplication
    for i in 0..num_blocks {
        let block_idx = if i < unique_blocks {
            i
        } else {
            i % unique_blocks
        };
        file.write_all(&unique_data[block_idx]).unwrap();
    }

    file.flush().unwrap();
    drop(file);
    file_path
}

// ---------------------------------------------------------------------------
// Memory result reporting
// ---------------------------------------------------------------------------

struct MemoryResults {
    label: &'static str,
    input_size: usize,
    rss_before: usize,
    rss_after: usize,
    peak_rss: usize,
    block_size: usize,
}

impl MemoryResults {
    fn rss_delta(&self) -> usize {
        self.rss_after.saturating_sub(self.rss_before)
    }

    /// Theoretical dedup map memory: unique_blocks * 48 bytes.
    fn estimated_dedup_map_bytes(&self) -> usize {
        let unique_blocks = self.input_size / self.block_size;
        unique_blocks * 48
    }

    fn print(&self) {
        let delta = self.rss_delta();
        let estimate = self.estimated_dedup_map_bytes();
        eprintln!(
            "  {}: input={:.1} MB | RSS before={:.1} MB, after={:.1} MB, delta={:.2} MB | peak={:.1} MB | dedup map estimate={:.2} MB",
            self.label,
            self.input_size as f64 / 1_048_576.0,
            self.rss_before as f64 / 1_048_576.0,
            self.rss_after as f64 / 1_048_576.0,
            delta as f64 / 1_048_576.0,
            self.peak_rss as f64 / 1_048_576.0,
            estimate as f64 / 1_048_576.0,
        );
    }
}

// ---------------------------------------------------------------------------
// Benchmark functions
// ---------------------------------------------------------------------------

fn bench_pack_memory_50mb(c: &mut Criterion) {
    let size: usize = 50_000_000; // 50 MB
    let duplication_pct = 0.25;
    let block_size: usize = 65536;

    eprintln!("\n=== Pack Memory: 50 MB (25% duplication) ===");

    let mut results: Option<MemoryResults> = None;

    let mut group = c.benchmark_group("PackMemory-50MB");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("pack", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_test_file_with_duplication(size, duplication_pct, &temp_dir);
                let rss_before = get_current_rss();
                (temp_dir, input, rss_before)
            },
            |(temp_dir, input, rss_before)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output,
                    "lz4".to_string(),
                    false,
                    false,
                    block_size as u32,
                    false,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let rss_after = get_current_rss();
                let peak_rss = get_peak_rss();

                if results.is_none() {
                    results = Some(MemoryResults {
                        label: "50 MB",
                        input_size: size,
                        rss_before,
                        rss_after,
                        peak_rss,
                        block_size,
                    });
                }

                black_box(rss_after);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    if let Some(r) = &results {
        r.print();
    }
}

fn bench_pack_memory_200mb(c: &mut Criterion) {
    let size: usize = 200_000_000; // 200 MB
    let duplication_pct = 0.25;
    let block_size: usize = 65536;

    eprintln!("\n=== Pack Memory: 200 MB (25% duplication) ===");

    let mut results: Option<MemoryResults> = None;

    let mut group = c.benchmark_group("PackMemory-200MB");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("pack", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_test_file_with_duplication(size, duplication_pct, &temp_dir);
                let rss_before = get_current_rss();
                (temp_dir, input, rss_before)
            },
            |(temp_dir, input, rss_before)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output,
                    "lz4".to_string(),
                    false,
                    false,
                    block_size as u32,
                    false,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let rss_after = get_current_rss();
                let peak_rss = get_peak_rss();

                if results.is_none() {
                    results = Some(MemoryResults {
                        label: "200 MB",
                        input_size: size,
                        rss_before,
                        rss_after,
                        peak_rss,
                        block_size,
                    });
                }

                black_box(rss_after);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    if let Some(r) = &results {
        r.print();
    }
}

fn bench_pack_memory_500mb(c: &mut Criterion) {
    let size: usize = 500_000_000; // 500 MB
    let duplication_pct = 0.25;
    let block_size: usize = 65536;

    eprintln!("\n=== Pack Memory: 500 MB (25% duplication) ===");

    let mut results: Option<MemoryResults> = None;

    let mut group = c.benchmark_group("PackMemory-500MB");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("pack", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_test_file_with_duplication(size, duplication_pct, &temp_dir);
                let rss_before = get_current_rss();
                (temp_dir, input, rss_before)
            },
            |(temp_dir, input, rss_before)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output,
                    "lz4".to_string(),
                    false,
                    false,
                    block_size as u32,
                    false,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let rss_after = get_current_rss();
                let peak_rss = get_peak_rss();

                if results.is_none() {
                    results = Some(MemoryResults {
                        label: "500 MB",
                        input_size: size,
                        rss_before,
                        rss_after,
                        peak_rss,
                        block_size,
                    });
                }

                black_box(rss_after);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    if let Some(r) = &results {
        r.print();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10));
    targets = bench_pack_memory_50mb, bench_pack_memory_200mb, bench_pack_memory_500mb
}
criterion_main!(benches);
