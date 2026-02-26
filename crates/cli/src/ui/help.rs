use clap::Command;

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";

pub struct Printer {
    cmd: Command,
}

impl Printer {
    pub fn new(cmd: Command) -> Self {
        Self { cmd }
    }

    /// Prints the top-level help menu (categories and list of commands)
    pub fn print_help(&mut self) {
        let bin_name = self.cmd.get_bin_name().unwrap_or("hexz").to_string();

        println!(
            "{}Usage:{} {}{}{} {}[OPTIONS]{} {}COMMAND{}",
            BOLD, RESET, GREEN, bin_name, RESET, CYAN, RESET, YELLOW, RESET
        );
        println!();
        if let Some(about) = self.cmd.get_about() {
            println!("{}", about);
        }
        println!();

        let mut archive_cmds = Vec::new();
        let mut vm_cmds = Vec::new();
        let mut sys_cmds = Vec::new();
        let mut other_cmds = Vec::new();

        let subcommands: Vec<Command> = self.cmd.get_subcommands().cloned().collect();

        for sub in subcommands {
            let name = sub.get_name().to_string();
            if name == "help" {
                continue;
            }

            let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
            let item = (name.clone(), about);

            match name.as_str() {
                "pack" | "inspect" | "diff" | "ls" | "build" | "convert" => archive_cmds.push(item),
                "boot" | "install" | "snap" | "commit" | "mount" | "unmount" => vm_cmds.push(item),
                "doctor" | "serve" | "keygen" | "sign" | "verify" => sys_cmds.push(item),
                _ => other_cmds.push(item),
            }
        }

        self.print_section("Archive Operations", archive_cmds);
        self.print_section("Virtual Machine Operations", vm_cmds);
        self.print_section("System & Diagnostics", sys_cmds);

        if !other_cmds.is_empty() {
            self.print_section("Other Commands", other_cmds);
        }

        println!("{}Options:{}", BOLD, RESET);
        println!("  {}{:<15}{} Print help", GREEN, "-h, --help", RESET);
        println!("  {}{:<15}{} Print version", GREEN, "-V, --version", RESET);
        println!();
        println!(
            "Run '{}{}{} COMMAND --help{}' for more information on a command.",
            BOLD, YELLOW, bin_name, RESET
        );
    }

    fn print_section(&self, header: &str, cmds: Vec<(String, String)>) {
        if cmds.is_empty() {
            return;
        }

        println!("{}{}{}:{}", BOLD, YELLOW, header, RESET);

        for (name, about) in cmds {
            println!("  {}{:<12}{} {}", GREEN, name, RESET, about);
        }
        println!();
    }

    /// Prints detailed help for a specific subcommand
    pub fn print_subcommand_help(&mut self, sub_name: &str) {
        let sub = match self.cmd.find_subcommand(sub_name) {
            Some(s) => s,
            None => return,
        };

        let bin_name = self.cmd.get_bin_name().unwrap_or("hexz");

        // 1. Usage
        println!(
            "{}Usage:{} {} {} {} {} {}[OPTIONS] [ARGS]{}",
            BOLD, RESET, GREEN, bin_name, sub_name, RESET, CYAN, RESET
        );
        println!();

        // 2. Detailed Description (long_about)
        if let Some(about) = sub.get_long_about().or_else(|| sub.get_about()) {
            println!("{}", about);
        }
        println!();

        // Collect all arguments
        let args: Vec<_> = sub.get_arguments().collect();

        // Partition into Positionals (Arguments) and Options (Flags)
        // Robust check: Positionals are arguments that have NO short flag AND NO long flag.
        let (mut positionals, mut flags): (Vec<_>, Vec<_>) = args
            .into_iter()
            .filter(|a| a.get_id() != "help" && a.get_id() != "version")
            .partition(|a| a.get_short().is_none() && a.get_long().is_none());

        // Sort positionals by index (so SOURCE comes before OUTPUT)
        // If index is missing, we push it to the end.
        positionals.sort_by_key(|a| a.get_index().unwrap_or(usize::MAX));

        // Sort flags alphabetically
        flags.sort_by(|a, b| a.get_id().cmp(b.get_id()));

        // 3. Arguments Section (Positional)
        if !positionals.is_empty() {
            println!("{}Arguments:{}", BOLD, RESET);
            for arg in positionals {
                let name = arg.get_id().as_str().to_uppercase();
                let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();

                // Check if required
                let required_note = if arg.is_required_set() {
                    format!("{} (required){}", YELLOW, RESET)
                } else {
                    String::new()
                };

                println!("  {}{:<28}{} {}{}", GREEN, name, RESET, help, required_note);
            }
            println!();
        }

        // 4. Options Section (Flags)
        println!("{}Options:{}", BOLD, RESET);

        for arg in flags {
            let short = arg
                .get_short()
                .map(|s| format!("-{},", s))
                .unwrap_or_default();
            let long = arg
                .get_long()
                .map(|l| format!("--{}", l))
                .unwrap_or_default();

            // Handle values like <OUTPUT>
            let value = if arg.get_action().takes_values() {
                let val_name = arg
                    .get_value_names()
                    .and_then(|names| names.first())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "VAL".to_string());
                format!(" <{}>", val_name.to_uppercase())
            } else {
                String::new()
            };

            let flag_str = format!("{} {}{}", short, long, value);
            let help_text = arg.get_help().map(|h| h.to_string()).unwrap_or_default();

            let required_note = if arg.is_required_set() {
                format!("{} (required){}", YELLOW, RESET)
            } else {
                String::new()
            };

            println!(
                "  {}{:<28}{} {}{}",
                GREEN,
                flag_str.trim(),
                RESET,
                help_text,
                required_note
            );
        }

        // Always show help flag
        println!("  {}{:<28}{} Print help", GREEN, "-h, --help", RESET);
        println!();

        // 5. Example Usage
        println!("{}Example:{}", BOLD, RESET);
        if let Some(example) = sub.get_after_help() {
            println!("  {}", example);
        } else {
            println!("  {} {} [flags] [args]", bin_name, sub_name);
        }
        println!();
    }
}
