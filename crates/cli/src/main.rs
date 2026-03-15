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
            let cmd = Cli::command();
            let args: Vec<String> = std::env::args().collect();
            if args.len() > 1 {
                let sub_name = &args[1];
                if cmd.find_subcommand(sub_name).is_some() {
                    use clap::error::ErrorKind;
                    match e.kind() {
                        ErrorKind::DisplayHelp
                        | ErrorKind::MissingRequiredArgument
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                            let mut printer = Printer::new(cmd);
                            printer.print_subcommand_help(sub_name);
                            std::process::exit(0);
                        }
                        ErrorKind::UnknownArgument => {
                            let has_help_flag = args.iter().any(|a| a == "--help" || a == "-h");
                            if has_help_flag {
                                let mut printer = Printer::new(cmd);
                                printer.print_subcommand_help(sub_name);
                                std::process::exit(0);
                            }
                            e.exit();
                        }
                        _ => {
                            e.exit();
                        }
                    }
                }
            }
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::MissingRequiredArgument
            {
                let mut printer = Printer::new(cmd);
                printer.print_help();
                std::process::exit(0);
            }
            e.exit();
        }
    };

    if cli.help {
        let mut printer = Printer::new(Cli::command());
        printer.print_help();
        return Ok(());
    }

    let command = match cli.command {
        Some(c) => c,
        None => {
            let mut printer = Printer::new(Cli::command());
            printer.print_help();
            return Ok(());
        }
    };

    match command {
        Commands::Pack {
            input,
            output,
            base,
            compression,
            encrypt,
            block_size,
            workers,
            dcam,
            dcam_optimal,
            silent,
        } => hexz_cli::cmd::data::pack::run(
            Some(input),
            base,
            output,
            compression,
            encrypt,
            false, // train_dict
            block_size,
            None,
            None,
            None,
            workers,
            dcam || dcam_optimal,
            dcam_optimal,
            silent,
        ),

        Commands::Extract { input, output } => {
            hexz_cli::cmd::data::extract::run(input, Some(output))
        }

        Commands::Show { snap, json } => hexz_cli::cmd::data::inspect::run(snap, json),

        Commands::Diff { a, b } => hexz_cli::cmd::data::diff::run(a, b),

        Commands::Log { dir } => hexz_cli::cmd::data::ls::run(dir),

        Commands::Convert {
            format,
            input,
            output,
            compression,
            block_size,
            silent,
        } => hexz_cli::cmd::data::convert::run(
            format,
            input,
            output,
            compression,
            block_size,
            silent,
        ),

        Commands::Predict {
            path,
            block_size,
            min_chunk,
            avg_chunk,
            max_chunk,
            json,
        } => hexz_cli::cmd::data::predict::run(
            path, block_size, min_chunk, avg_chunk, max_chunk, json,
        ),

        #[cfg(feature = "fuse")]
        Commands::Mount {
            snap,
            mountpoint,
            overlay,
            editable,
            daemon,
            cache_size,
            uid,
            gid,
        } => hexz_cli::cmd::data::mount::run(
            snap, mountpoint, daemon, cache_size, uid, gid, overlay, editable, None,
        ),

        #[cfg(feature = "fuse")]
        Commands::Unmount { mountpoint } => hexz_cli::cmd::data::unmount::run(mountpoint),

        #[cfg(feature = "fuse")]
        Commands::Shell {
            snap,
            overlay,
            editable,
            cache_size,
        } => hexz_cli::cmd::data::shell::run(snap, overlay, editable, cache_size),

        #[cfg(feature = "fuse")]
        Commands::Commit {
            output,
            mountpoint,
            base,
        } => hexz_cli::cmd::data::commit::run(output, mountpoint, base),

        #[cfg(feature = "fuse")]
        Commands::Checkout { archive, path } => hexz_cli::cmd::data::checkout::run(archive, path),

        #[cfg(feature = "fuse")]
        Commands::Status { path } => hexz_cli::cmd::data::status::run(path),

        #[cfg(feature = "fuse")]
        Commands::Init { path } => hexz_cli::cmd::data::init::run(path),

        #[cfg(feature = "fuse")]
        Commands::Remote { action } => hexz_cli::cmd::data::remote::run(action),

        #[cfg(feature = "fuse")]
        Commands::Push { remote, archive } => hexz_cli::cmd::data::push::run(remote, archive),

        #[cfg(feature = "fuse")]
        Commands::Pull { remote } => hexz_cli::cmd::data::pull::run(remote),

        #[cfg(feature = "server")]
        Commands::Serve {
            snap,
            port,
            bind,
            daemon,
        } => hexz_cli::cmd::sys::serve::run(snap, port, bind, daemon, false),

        #[cfg(feature = "signing")]
        Commands::Keygen { output_dir } => hexz_cli::cmd::sys::keygen::run(output_dir),

        #[cfg(feature = "signing")]
        Commands::Sign { key, image } => hexz_cli::cmd::sys::sign::run(key, image),

        #[cfg(feature = "signing")]
        Commands::Verify { key, image } => hexz_cli::cmd::sys::verify::run(key, image),

        Commands::Doctor => hexz_cli::cmd::sys::doctor::run(),
    }
}
