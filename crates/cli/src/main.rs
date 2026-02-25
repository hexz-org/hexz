//! Entry point for the `hexz` CLI binary.

use clap::{CommandFactory, Parser};
use hexz_cli::args::{Cli, Commands};
use hexz_cli::ui::help::Printer;

fn main() -> anyhow::Result<()> {
    hexz_common::logging::init();

    // 1. Attempt to parse arguments
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // 2. Intercept errors to provide custom help
            let cmd = Cli::command();
            let args: Vec<String> = std::env::args().collect();

            // Check if user provided a subcommand (e.g., "hexz pack ...")
            if args.len() > 1 {
                let sub_name = &args[1];

                // If the first arg is a valid subcommand...
                if cmd.find_subcommand(sub_name).is_some() {
                    use clap::error::ErrorKind;

                    // ...and the error is specifically about help or missing required args
                    match e.kind() {
                        ErrorKind::DisplayHelp
                        | ErrorKind::MissingRequiredArgument
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                            let mut printer = Printer::new(cmd);
                            printer.print_subcommand_help(sub_name);
                            std::process::exit(0);
                        }
                        ErrorKind::UnknownArgument => {
                            // --help is disabled globally but users expect `hexz <cmd> --help`
                            let has_help_flag = args.iter().any(|a| a == "--help" || a == "-h");
                            if has_help_flag {
                                let mut printer = Printer::new(cmd);
                                printer.print_subcommand_help(sub_name);
                                std::process::exit(0);
                            }
                            e.exit();
                        }
                        _ => {
                            // If it's a legitimate error (e.g. invalid int), show clap error
                            e.exit();
                        }
                    }
                }
            }

            // If no subcommand was found, or just "hexz --help", show top-level help
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::MissingRequiredArgument
            {
                let mut printer = Printer::new(cmd);
                printer.print_help();
                std::process::exit(0);
            }

            // Fallback for other errors
            e.exit();
        }
    };

    // 3. Handle explicit --help flag if it somehow passed parsing (Action::SetTrue)
    if cli.help {
        let mut printer = Printer::new(Cli::command());
        printer.print_help();
        return Ok(());
    }

    // 4. Determine command to run
    let command = match cli.command {
        Some(c) => c,
        None => {
            let mut printer = Printer::new(Cli::command());
            printer.print_help();
            return Ok(());
        }
    };

    // 5. Execute command
    match command {
        // --------------------------------------------------------------------
        // Archive Operations
        // --------------------------------------------------------------------
        Commands::Pack {
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

        Commands::Inspect { snap, json } => hexz_cli::cmd::data::inspect::run(snap, json),

        #[cfg(feature = "diagnostics")]
        Commands::Diff {
            overlay,
            blocks,
            files,
        } => hexz_cli::cmd::data::diff::run(overlay, blocks, files),

        Commands::Build {
            source,
            memory,
            output,
            profile,
            encrypt,
            cdc,
        } => hexz_cli::cmd::data::build::run(source, memory, output, profile, encrypt, cdc),

        #[cfg(feature = "diagnostics")]
        Commands::Analyze { input } => hexz_cli::cmd::data::analyze::run(input),

        Commands::Convert {
            format,
            input,
            output,
            compression,
            block_size,
            profile,
            silent,
        } => hexz_cli::cmd::data::convert::run(
            format,
            input,
            output,
            compression,
            block_size,
            profile,
            silent,
        ),

        // --------------------------------------------------------------------
        // Virtual Machine Operations
        // --------------------------------------------------------------------
        #[cfg(feature = "fuse")]
        Commands::Boot {
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
        Commands::Install {
            iso,
            primary_size,
            ram,
            output,
            no_graphics,
            vnc,
            cdc,
        } => hexz_cli::cmd::vm::install::run(iso, primary_size, ram, output, no_graphics, vnc, cdc),

        #[cfg(unix)]
        Commands::Snap {
            socket,
            base,
            overlay,
            output,
        } => hexz_cli::cmd::vm::snap::run(socket, overlay, base, output),

        Commands::Commit {
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
        Commands::Mount {
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
        Commands::Unmount { mountpoint } => hexz_cli::cmd::vm::unmount::run(mountpoint),

        // --------------------------------------------------------------------
        // System & Diagnostics
        // --------------------------------------------------------------------
        #[cfg(feature = "diagnostics")]
        Commands::Doctor => hexz_cli::cmd::sys::doctor::run(),

        #[cfg(feature = "diagnostics")]
        Commands::Bench {
            image,
            block_size,
            duration,
            threads,
        } => hexz_cli::cmd::sys::bench::run(image, block_size, duration, threads),

        #[cfg(feature = "server")]
        Commands::Serve {
            snap,
            port,
            daemon,
            nbd,
            s3,
        } => hexz_cli::cmd::sys::serve::run(snap, port, daemon, nbd, s3),

        #[cfg(feature = "signing")]
        Commands::Keygen { output_dir } => hexz_cli::cmd::sys::keygen::run(output_dir),

        #[cfg(feature = "signing")]
        Commands::Sign { key, image } => hexz_cli::cmd::sys::sign::run(key, image),

        #[cfg(feature = "signing")]
        Commands::Verify { key, image } => hexz_cli::cmd::sys::verify::run(key, image),
    }
}
