//! 常用命令的参数角色；数据发现留给应用层，shell 方言只决定已知别名的语义。

use super::*;

const FILES: Source = Source::Paths { directories_only: false };
const DIRS: Source = Source::Paths { directories_only: true };
const SSH: &[OptionSpec] = &[
    flag(&["-4", "-6", "-A", "-a", "-C", "-f", "-G", "-g", "-K", "-k", "-M", "-N", "-n", "-q", "-s", "-T", "-t", "-V", "-v", "-X", "-x", "-Y", "-y"]),
    value(&["-F", "-i", "-E", "-S"], FILES),
    value(&["-J"], Source::SshHosts { jump: true }),
    value(&["-B", "-b", "-c", "-D", "-I", "-L", "-l", "-m", "-O", "-o", "-p", "-Q", "-R", "-W", "-w"], Source::None),
];
const WSL: &[OptionSpec] = &[
    flag(&["--list", "-l", "--verbose", "-v", "--quiet", "-q", "--all", "--running", "--status", "--version", "--help", "--shutdown", "--update"]),
    value(&["--distribution", "-d", "--set-default", "-s", "--terminate", "-t", "--unregister", "--export", "--set-version"], Source::WslDistributions),
    value(&["--user", "-u", "--cd", "--install", "--import"], Source::None),
    value(&["--shell-type"], Source::Words(&["standard", "login", "none"])),
    flag(&["--exec", "-e", "--"]),
];
const POSIX_LS: &[OptionSpec] = &[flag(&["-a", "-A", "-l", "-h", "-R", "-d", "-F", "-t", "-r", "-S", "-1"])];
const POSIX_CAT: &[OptionSpec] = &[flag(&["-b", "-e", "-n", "-s", "-t", "-u", "-v"])];
const POSIX_COPY: &[OptionSpec] = &[flag(&["-R", "-r", "-f", "-i", "-p", "-v"])];
const POSIX_MOVE: &[OptionSpec] = &[flag(&["-f", "-i", "-n", "-v"])];
const POSIX_REMOVE: &[OptionSpec] = &[flag(&["-f", "-i", "-r", "-R", "-v"])];
const POSIX_MKDIR: &[OptionSpec] = &[flag(&["-p", "-v"]), value(&["-m"], Source::None)];
const POSIX_GREP: &[OptionSpec] = &[
    flag(&["-i", "-n", "-r", "-R", "-v", "-l", "-c", "-E", "-F", "-w", "-x", "-q"]),
    value(&["-e"], Source::None), value(&["-f"], FILES),
];
const PS_LIST: &[OptionSpec] = &[
    value(&["-Path", "-LiteralPath"], FILES),
    value(&["-Filter", "-Include", "-Exclude", "-Depth"], Source::None),
    flag(&["-Directory", "-File", "-Force", "-Recurse", "-Name", "-Hidden"]),
];
const PS_CONTENT: &[OptionSpec] = &[
    value(&["-Path", "-LiteralPath"], FILES),
    value(&["-TotalCount", "-Tail", "-ReadCount", "-Encoding", "-Delimiter"], Source::None),
    flag(&["-Raw", "-Wait", "-Force"]),
];
const PS_COPY: &[OptionSpec] = &[
    value(&["-Path", "-LiteralPath", "-Destination"], FILES),
    flag(&["-Recurse", "-Force", "-PassThru", "-WhatIf", "-Confirm"]),
];
const PS_MOVE: &[OptionSpec] = &[
    value(&["-Path", "-LiteralPath", "-Destination"], FILES),
    flag(&["-Force", "-PassThru", "-WhatIf", "-Confirm"]),
];
const PS_REMOVE: &[OptionSpec] = &[
    value(&["-Path", "-LiteralPath"], FILES),
    flag(&["-Force", "-Recurse", "-WhatIf", "-Confirm"]),
];
const PS_CD: &[OptionSpec] = &[value(&["-Path", "-LiteralPath"], DIRS), flag(&["-PassThru"])];

impl Context {
    pub(super) fn common(&mut self, syntax: ShellSyntax) -> Option<Source> {
        let program = self.input.arguments[0].trim_end_matches(".exe");
        let ssh = program == "ssh";
        let wsl = program == "wsl";
        let powershell = syntax == ShellSyntax::PowerShell;
        let program_lower = program.to_ascii_lowercase();
        let (options, mut source, limit) = if ssh {
            (SSH, Source::SshHosts { jump: false }, 1)
        } else if wsl {
            (WSL, Source::None, usize::MAX)
        } else if powershell {
            match program_lower.as_str() {
                "cd" | "chdir" | "set-location" => (PS_CD, DIRS, 1),
                "ls" | "dir" | "gci" | "get-childitem" => (PS_LIST, FILES, usize::MAX),
                "cat" | "gc" | "type" | "get-content" => (PS_CONTENT, FILES, usize::MAX),
                "cp" | "copy" | "copy-item" => (PS_COPY, FILES, usize::MAX),
                "mv" | "move" | "move-item" => (PS_MOVE, FILES, usize::MAX),
                "rm" | "del" | "erase" | "remove-item" => (PS_REMOVE, FILES, usize::MAX),
                _ => return None,
            }
        } else {
            match program {
                "cd" => (&[][..], DIRS, 1),
                "ls" if syntax == ShellSyntax::Posix => (POSIX_LS, FILES, usize::MAX),
                "cat" if syntax == ShellSyntax::Posix => (POSIX_CAT, FILES, usize::MAX),
                "cp" if syntax == ShellSyntax::Posix => (POSIX_COPY, FILES, usize::MAX),
                "mv" if syntax == ShellSyntax::Posix => (POSIX_MOVE, FILES, usize::MAX),
                "rm" if syntax == ShellSyntax::Posix => (POSIX_REMOVE, FILES, usize::MAX),
                "mkdir" if syntax == ShellSyntax::Posix => (POSIX_MKDIR, DIRS, usize::MAX),
                "grep" if syntax == ShellSyntax::Posix => (POSIX_GREP, Source::None, usize::MAX),
                // 未知远端 shell 仅共享字面路径，不猜测它的别名和选项方言。
                "ls" | "cat" | "cp" | "mv" | "rm" | "mkdir" if syntax == ShellSyntax::Literal => (&[][..], if program == "mkdir" { DIRS } else { FILES }, usize::MAX),
                _ => return None,
            }
        };
        self.options = options;
        let mut index = 1;
        let mut positional = 0;
        let mut parse_options = true;
        let mut grep_pattern = false;
        while let Some(arg) = self.input.arguments.get(index) {
            if parse_options && arg == "--" {
                if wsl { return Some(Source::None); }
                parse_options = false;
            } else if parse_options && arg.starts_with('-') {
                if wsl && matches!(arg.as_str(), "--exec" | "-e") { return Some(Source::None); }
                if let Some((option, attached)) = resolve(options, arg, powershell && !ssh && !wsl) {
                    let name = arg.split('=').next().unwrap_or(arg);
                    if let Some(value_source) = option.value {
                        let value = if let Some(offset) = attached {
                            &arg[offset..]
                        } else {
                            index += 1;
                            let Some(value) = self.input.arguments.get(index) else { return Some(value_source) };
                            value.as_str()
                        };
                        if ssh && arg.starts_with("-F") { self.ssh_config = Some(value.to_owned()); }
                        if program == "grep" && matches!(name, "-e" | "-f") { grep_pattern = true; }
                        if wsl {
                            source = match name {
                                "--export" => FILES,
                                "--set-version" => Source::Words(&["1", "2"]),
                                "--import" => DIRS,
                                _ => source,
                            };
                        }
                    } else if attached.is_some() { return None; }
                } else if !arg.starts_with("--") && arg[1..].chars().all(|ch| {
                    options.iter().any(|option| option.value.is_none() && option.names.iter().any(|name| name.len() == 2 && name.ends_with(ch)))
                }) {
                    // 常见 -al/-vvv 只在每个成员均为无值选项时成立。
                } else { return None; }
            } else {
                positional += 1;
                if ssh || positional >= limit { return Some(Source::None); }
                if program == "grep" { grep_pattern = true; }
                if wsl { source = if matches!(source, Source::Paths { directories_only: true }) { FILES } else { Source::None }; }
            }
            index += 1;
        }
        if parse_options && self.input.prefix().starts_with('-') {
            if let Some((option, Some(offset))) = resolve(options, self.input.prefix(), powershell && !ssh && !wsl) {
                self.attached = Some(offset);
                return option.value;
            }
            return Some(Source::Options);
        }
        if program == "grep" && grep_pattern { source = FILES; }
        Some(source)
    }
}

fn resolve<'a>(options: &'a [OptionSpec], arg: &str, case_insensitive: bool) -> Option<(&'a OptionSpec, Option<usize>)> {
    for option in options {
        for name in option.names {
            if arg == *name || case_insensitive && arg.eq_ignore_ascii_case(name) { return Some((option, None)); }
            if let Some(rest) = arg.strip_prefix(name) {
                if rest.starts_with('=') { return Some((option, Some(name.len() + 1))); }
                if name.len() == 2 && !name.starts_with("--") && !rest.is_empty() && option.value.is_some() { return Some((option, Some(2))); }
            }
        }
    }
    None
}
