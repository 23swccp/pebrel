//! Saved launch identities may retain a previous build's temporary bootstrap path.

use std::path::{Component, Path, PathBuf, Prefix};

use crate::tty::Shell;

pub(super) fn refresh(shell: &Shell) -> Option<Shell> {
    let script = prepared_script(shell)?;
    let mut directories = vec![std::env::temp_dir()];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        directories.push(PathBuf::from(local).join("Temp"));
    }
    if let Some(config) = std::env::var_os("PEBREL_CONFIG_DIR") {
        directories.push(PathBuf::from(config).join("shell-integration"));
    }
    let args = original_args(shell, &script, &directories)?;
    // 只更新本次启动参数，不覆盖旧路径；新进程必须加载当前构建生成的集成。
    let current = super::nebula_prompt_script_path()?;
    Some(Shell::new(shell.program().to_owned(), super::powershell_integration_args(args, &current)))
}

fn prepared_script(shell: &Shell) -> Option<PathBuf> {
    let name = Path::new(shell.program()).file_name()?.to_str()?;
    if !["powershell", "powershell.exe", "pwsh", "pwsh.exe"]
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        return None;
    }
    let args = shell.args();
    let split = args.len().checked_sub(5)?;
    if !args[split..split + 4]
        .iter()
        .zip(["-NoExit", "-ExecutionPolicy", "Bypass", "-Command"])
        .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        || !args[..split].iter().all(|arg| {
            ["-NoLogo", "-NoProfile", "-NoExit", "-STA", "-MTA"]
                .iter()
                .any(|flag| arg.eq_ignore_ascii_case(flag))
        })
    {
        return None;
    }
    let literal = args.last()?.strip_prefix(". '")?.strip_suffix('\'')?;
    if literal.contains(['\'', '\r', '\n']) {
        return None;
    }
    let path = PathBuf::from(literal);
    // 不探测 UNC 路径，也不把同名的任意用户脚本当作托管文件。
    let local = matches!(path.components().next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)));
    (local
        && path.is_absolute()
        && path.file_name()?.to_str()?.eq_ignore_ascii_case("pebrel_prompt.ps1"))
    .then_some(path)
}

fn original_args(shell: &Shell, script: &Path, directories: &[PathBuf]) -> Option<Vec<String>> {
    let parent = script.parent()?.canonicalize().ok()?;
    let owned = directories.iter().any(|directory| {
        directory.is_absolute()
            && directory.canonicalize().is_ok_and(|directory| directory == parent)
    });
    owned.then(|| shell.args()[..shell.args().len() - 5].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Directories(PathBuf);

    impl Directories {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "pebrel-bootstrap-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&root).unwrap();
            for name in ["old", "current"] {
                std::fs::create_dir(root.join(name)).unwrap();
            }
            Self(root)
        }
    }

    impl Drop for Directories {
        fn drop(&mut self) {
            for name in ["old", "current"] {
                let _ = std::fs::remove_dir(self.0.join(name));
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    fn prepared(program: &str, directory: &Path) -> Shell {
        Shell::new(
            program.to_owned(),
            super::super::powershell_integration_args(
                vec!["-NoLogo".into(), "-NoProfile".into()],
                &directory.join("pebrel_prompt.ps1"),
            ),
        )
    }

    #[test]
    fn frozen_managed_launch_can_use_the_current_bootstrap_without_changing_user_flags() {
        let directories = Directories::new();
        let old = directories.0.join("old");
        let current = directories.0.join("current");
        let shell = prepared("powershell.exe", &old);
        let path = prepared_script(&shell).unwrap();
        let args = original_args(&shell, &path, &[old.clone()]).unwrap();
        assert_eq!(args, ["-NoLogo", "-NoProfile"]);
        let refreshed =
            super::super::powershell_integration_args(args, &current.join("pebrel_prompt.ps1"));
        assert_eq!(&refreshed[..2], &shell.args()[..2]);
        assert!(!refreshed.last().unwrap().contains(&old.to_string_lossy().to_string()));
        assert!(refreshed.last().unwrap().contains(&current.to_string_lossy().to_string()));
        assert!(!old.join("pebrel_prompt.ps1").exists(), "old cache is not written");
    }

    #[test]
    fn custom_commands_shells_and_same_named_files_outside_managed_roots_are_untouched() {
        let directories = Directories::new();
        let managed = directories.0.join("old");
        let custom = directories.0.join("current");
        for program in ["cmd.exe", "wsl.exe", "bash.exe"] {
            assert!(prepared_script(&prepared(program, &managed)).is_none());
        }
        let shell = prepared("pwsh.exe", &custom);
        let script = prepared_script(&shell).unwrap();
        assert!(original_args(&shell, &script, &[managed.clone()]).is_none());
        for command in [
            "Write-Output test",
            ". '\\\\server\\share\\pebrel_prompt.ps1'",
            ". 'C:\\Temp\\custom.ps1'",
            ". 'C:\\Temp\\pebrel_prompt.ps1'; Write-Output test",
        ] {
            let mut args = prepared("powershell.exe", &managed).args().to_vec();
            *args.last_mut().unwrap() = command.into();
            assert!(prepared_script(&Shell::new("powershell.exe".into(), args)).is_none());
        }
        let mut args = shell.args().to_vec();
        args.insert(0, "-File".into());
        assert!(prepared_script(&Shell::new("powershell.exe".into(), args)).is_none());
    }
}
