use active_win_pos_rs::get_active_window;
use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// VS Code / IDE style: `"file.rs - Project - Editor"` → leading segment as file hint.
pub(crate) fn file_hint_from_window_title(title: &str) -> Option<String> {
    if title.contains(" - ") {
        title.split(" - ").next().map(|s| s.trim().to_string())
    } else {
        None
    }
}

fn git_command() -> Command {
    let mut c = Command::new("git");
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct SystemContext {
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_name: Option<String>,
    pub file_path: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct GitContext {
    pub branch: Option<String>,
    pub last_commit_msg: Option<String>,
    pub is_dirty: bool,
}

pub fn get_system_context() -> SystemContext {
    match get_active_window() {
        Ok(window) => {
            let app = Some(window.app_name);
            let title = Some(window.title.clone());
            
            // Heuristic: Extract filename from title
            // VS Code: "filename.rs - Project - VS Code"
            // IntelliJ: "filename.rs [Project] - ..."
            let file_name = title.as_ref().and_then(|t| file_hint_from_window_title(t));

            SystemContext {
                app_name: app,
                window_title: title,
                file_name,
                file_path: None, // Hard to get full path from title alone reliably
            }
        },
        Err(_) => SystemContext::default()
    }
}

pub fn get_git_context(cwd: &str) -> Option<GitContext> {
    let path = PathBuf::from(cwd);
    if !path.exists() { return None; }

    // 1. Get Branch
    let branch = git_command()
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    // If not a git repo, return None
    if branch.is_none() { return None; }

    // 2. Check Dirty Status
    let is_dirty = git_command()
        .args(["status", "--porcelain"])
        .current_dir(&path)
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    // 3. Last Commit
    let last_commit_msg = git_command()
        .args(["log", "-1", "--pretty=%B"])
        .current_dir(&path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    Some(GitContext {
        branch,
        last_commit_msg,
        is_dirty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_hint_splits_vscode_style_title() {
        assert_eq!(
            file_hint_from_window_title("main.rs - flowsight - VS Code").as_deref(),
            Some("main.rs")
        );
    }

    #[test]
    fn file_hint_none_without_separator() {
        assert_eq!(file_hint_from_window_title("YouTube"), None);
    }
}
