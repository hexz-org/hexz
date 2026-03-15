use clap::Command;

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";

/// Custom help output printer for the Hexz CLI.
#[derive(Debug)]
pub struct Printer {
    cmd: Command,
}

impl Printer {
    /// Create a new `Printer` wrapping the given clap `Command`.
    pub const fn new(cmd: Command) -> Self {
        Self { cmd }
    }

    /// Prints the top-level help menu (categories and list of commands)
    pub fn print_help(&mut self) {
        let bin_name = self.cmd.get_bin_name().unwrap_or("hexz").to_string();

        println!(
            "{BOLD}Usage:{RESET} {GREEN}{bin_name}{RESET} {CYAN}[OPTIONS]{RESET} {YELLOW}COMMAND{RESET}"
        );
        println!();
        if let Some(about) = self.cmd.get_about() {
            println!("{about}");
        }
        println!();

        let mut create_cmds = Vec::new();
        let mut inspect_cmds = Vec::new();
        let mut network_cmds = Vec::new();
        let mut infra_cmds = Vec::new();

        let subcommands: Vec<Command> = self.cmd.get_subcommands().cloned().collect();

        for sub in subcommands {
            let name = sub.get_name().to_string();
            if name == "help" {
                continue;
            }

            let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
            let item = (name.clone(), about);

            match name.as_str() {
                "pack" | "extract" | "init" | "checkout" | "commit" | "status" => create_cmds.push(item),
                "inspect" | "show" | "diff" | "log" | "ls" | "predict" | "convert" => inspect_cmds.push(item),
                "mount" | "unmount" | "shell" | "serve" | "remote" | "push" | "pull" => network_cmds.push(item),
                "keygen" | "sign" | "verify" | "doctor" => infra_cmds.push(item),
                _ => {}
            }
        }

        self.print_section("core archive & workspace workflows", create_cmds);
        self.print_section("data inspection & conversion", inspect_cmds);
        self.print_section("networking & cloud collaboration", network_cmds);
        self.print_section("security & system health", infra_cmds);

        println!("{BOLD}Options:{RESET}");
        println!("  {GREEN}{:<15}{RESET} Print help", "-h, --help");
        println!("  {GREEN}{:<15}{RESET} Print version", "-V, --version");
        println!();
        println!(
            "Run '{BOLD}{YELLOW}{bin_name} COMMAND --help{RESET}' for more information on a command."
        );
    }

    #[allow(clippy::unused_self)]
    fn print_section(&self, header: &str, cmds: Vec<(String, String)>) {
        if cmds.is_empty() {
            return;
        }

        println!("{BOLD}{YELLOW}{header}:{RESET}");

        for (name, about) in cmds {
            println!("  {GREEN}{name:<12}{RESET} {about}");
        }
        println!();
    }

    /// Prints detailed help for a specific subcommand
    pub fn print_subcommand_help(&mut self, sub_name: &str) {
        let Some(sub) = self.cmd.find_subcommand(sub_name) else {
            return;
        };

        let bin_name = self.cmd.get_bin_name().unwrap_or("hexz");

        // 1. Usage
        println!(
            "{BOLD}Usage:{RESET} {GREEN} {bin_name} {sub_name} {RESET} {CYAN}[OPTIONS] [ARGS]{RESET}"
        );
        println!();

        // 2. Detailed Description (long_about)
        if let Some(about) = sub.get_long_about().or_else(|| sub.get_about()) {
            println!("{about}");
        }
        println!();

        // Collect all arguments
        // Partition into Positionals (Arguments) and Options (Flags)
        // Robust check: Positionals are arguments that have NO short flag AND NO long flag.
        let (mut positionals, mut flags): (Vec<_>, Vec<_>) = sub
            .get_arguments()
            .filter(|a| a.get_id() != "help" && a.get_id() != "version")
            .partition(|a| a.get_short().is_none() && a.get_long().is_none());

        // Sort positionals by index (so SOURCE comes before OUTPUT)
        // If index is missing, we push it to the end.
        positionals.sort_by_key(|a| a.get_index().unwrap_or(usize::MAX));

        // Sort flags alphabetically
        flags.sort_by(|a, b| a.get_id().cmp(b.get_id()));

        // 3. Arguments Section (Positional)
        if !positionals.is_empty() {
            println!("{BOLD}Arguments:{RESET}");
            for arg in positionals {
                let name = arg.get_id().as_str().to_uppercase();
                let help = arg.get_help().map(ToString::to_string).unwrap_or_default();

                // Check if required
                let required_note = if arg.is_required_set() {
                    format!("{YELLOW} (required){RESET}")
                } else {
                    String::new()
                };

                println!("  {GREEN}{name:<28}{RESET} {help}{required_note}");
            }
            println!();
        }

        // 4. Options Section (Flags)
        println!("{BOLD}Options:{RESET}");

        for arg in flags {
            let short = arg
                .get_short()
                .map(|s| format!("-{s},"))
                .unwrap_or_default();
            let long = arg
                .get_long()
                .map(|l| format!("--{l}"))
                .unwrap_or_default();

            // Handle values like <OUTPUT>
            let value = if arg.get_action().takes_values() {
                let val_name = arg
                    .get_value_names()
                    .and_then(|names| names.first())
                    .map_or_else(|| "VAL".to_string(), ToString::to_string);
                format!(" <{}>", val_name.to_uppercase())
            } else {
                String::new()
            };

            let flag_str = format!("{short} {long}{value}");
            let help_text = arg.get_help().map(ToString::to_string).unwrap_or_default();

            let required_note = if arg.is_required_set() {
                format!("{YELLOW} (required){RESET}")
            } else {
                String::new()
            };

            let trimmed = flag_str.trim();
            println!(
                "  {GREEN}{trimmed:<28}{RESET} {help_text}{required_note}"
            );
        }

        // Always show help flag
        println!("  {GREEN}{:<28}{RESET} Print help", "-h, --help");
        println!();

        // 5. Example Usage
        println!("{BOLD}Example:{RESET}");
        if let Some(example) = sub.get_after_help() {
            println!("  {example}");
        } else {
            println!("  {bin_name} {sub_name} [flags] [args]");
        }
        println!();
    }
}
