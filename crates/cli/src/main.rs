//! Entry point for the `hexz` CLI binary.

use clap::Parser;
use hexz_cli::args::{Cli, Commands, DataCommands, SysCommands, VmCommands};

fn main() -> anyhow::Result<()> {
    hexz_common::logging::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Data(data_cmd) => match data_cmd {
            DataCommands::Pack {
                disk,
                memory,
                output,
                compression,
                encrypt,
                train_dict,
                block_size,
                cdc,
                min_chunk,
                avg_chunk,
                max_chunk,
                silent,
            } => hexz_cli::cmd::data::pack::run(
                disk,
                memory,
                output,
                compression,
                encrypt,
                train_dict,
                block_size,
                cdc,
                min_chunk,
                avg_chunk,
                max_chunk,
                silent,
            ),

            DataCommands::Info { snap, json } => hexz_cli::cmd::data::info::run(snap, json),

            #[cfg(feature = "diagnostics")]
            DataCommands::Diff {
                overlay,
                blocks,
                files,
            } => hexz_cli::cmd::data::diff::run(overlay, blocks, files),

            DataCommands::Build {
                source,
                memory,
                output,
                profile,
                encrypt,
                cdc,
            } => hexz_cli::cmd::data::build::run(source, memory, output, profile, encrypt, cdc),

            #[cfg(feature = "diagnostics")]
            DataCommands::Analyze { input } => hexz_cli::cmd::data::analyze::run(input),
        },

        Commands::Vm(vm_cmd) => match vm_cmd {
            #[cfg(feature = "fuse")]
            VmCommands::Boot {
                snap,
                ram,
                no_kvm,
                network,
                backend,
                persist,
                qmp_socket,
                no_graphics,
                vnc,
            } => hexz_cli::cmd::vm::boot::run(
                snap,
                ram,
                !no_kvm,
                persist,
                qmp_socket,
                network,
                backend,
                no_graphics,
                vnc,
            ),

            #[cfg(feature = "fuse")]
            VmCommands::Install {
                iso,
                disk_size,
                ram,
                output,
                no_graphics,
                vnc,
                cdc,
            } => {
                hexz_cli::cmd::vm::install::run(iso, disk_size, ram, output, no_graphics, vnc, cdc)
            }

            VmCommands::Snap {
                socket,
                base,
                overlay,
                output,
            } => hexz_cli::cmd::vm::snap::run(socket, overlay, base, output),

            VmCommands::Commit {
                base,
                overlay,
                output,
                compression,
                block_size,
                keep_overlay,
                flatten: _,
                message,
                thin,
            } => hexz_cli::cmd::vm::commit::run(
                base,
                overlay,
                None,
                output,
                compression,
                block_size,
                keep_overlay,
                message,
                thin,
            ),

            #[cfg(feature = "fuse")]
            VmCommands::Mount {
                snap,
                mountpoint,
                overlay,
                daemon,
                rw,
                cache_size,
                uid,
                gid,
                nbd,
            } => hexz_cli::cmd::vm::mount::run(
                snap, mountpoint, overlay, daemon, rw, cache_size, uid, gid, nbd,
            ),

            #[cfg(feature = "fuse")]
            VmCommands::Unmount { mountpoint } => hexz_cli::cmd::vm::unmount::run(mountpoint),
        },

        Commands::Sys(sys_cmd) => match sys_cmd {
            #[cfg(feature = "diagnostics")]
            SysCommands::Doctor => hexz_cli::cmd::sys::doctor::run(),

            #[cfg(feature = "diagnostics")]
            SysCommands::Bench {
                image,
                block_size,
                duration,
                threads,
            } => hexz_cli::cmd::sys::bench::run(image, block_size, duration, threads),

            #[cfg(feature = "server")]
            SysCommands::Serve {
                snap,
                port,
                daemon,
                nbd,
                s3,
            } => hexz_cli::cmd::sys::serve::run(snap, port, daemon, nbd, s3),

            #[cfg(feature = "signing")]
            SysCommands::Keygen { output_dir } => hexz_cli::cmd::sys::keygen::run(output_dir),

            #[cfg(feature = "signing")]
            SysCommands::Sign { key, image } => hexz_cli::cmd::sys::sign::run(key, image),

            #[cfg(feature = "signing")]
            SysCommands::Verify { key, image } => hexz_cli::cmd::sys::verify::run(key, image),
        },
    }
}
