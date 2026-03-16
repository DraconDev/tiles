use crate::app::{CommitInfo, FileMetadata, GitPendingChange};
use crate::state::{RemoteBookmark, RemoteSession};
use dracon_system::{
    RemoteBookmark as RemoteBookmarkContract, RemoteConnectContract, RemoteConnectRequest,
    RemoteConnection, RemoteEntryMetadata, RemoteExecContract, RemoteFsContract,
    SshRemoteConnector, SshRemoteExecProvider, SshRemoteFsProvider,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Git data: (history, pending, branch, ahead, behind, summary, remotes, stashes)
type GitData = (
    Vec<CommitInfo>,
    Vec<GitPendingChange>,
    String,
    usize,
    usize,
    String,
    Vec<String>,
    Vec<String>,
);

pub fn connect_remote(bookmark: &RemoteBookmark) -> anyhow::Result<RemoteSession> {
    let connector = SshRemoteConnector;
    let request = RemoteConnectRequest {
        bookmark: RemoteBookmarkContract {
            name: bookmark.name.clone(),
            host: bookmark.host.clone(),
            user: bookmark.user.clone(),
            port: bookmark.port,
            key_path: bookmark.key_path.clone(),
        },
        timeout_ms: 8_000,
    };
    let connected = connector.connect(&request)?;
    Ok(RemoteSession {
        host: connected.host,
        user: connected.user,
        name: connected.name,
        port: connected.port,
        key_path: connected.key_path,
    })
}

pub fn read_dir_with_metadata(
    remote: &RemoteSession,
    path: &Path,
) -> std::io::Result<(Vec<PathBuf>, HashMap<PathBuf, FileMetadata>)> {
    let provider = SshRemoteFsProvider;
    let connection = to_connection(remote);
    let (files, metadata) = provider.read_dir_with_metadata(&connection, path)?;
    Ok((
        files,
        metadata
            .into_iter()
            .map(|(k, v)| (k, map_metadata(v)))
            .collect(),
    ))
}

pub fn read_to_string(remote: &RemoteSession, path: &Path) -> std::io::Result<String> {
    let provider = SshRemoteFsProvider;
    provider.read_to_string(&to_connection(remote), path)
}

pub fn write_string(remote: &RemoteSession, path: &Path, content: &str) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.write_string(&to_connection(remote), path, content)
}

pub fn create_file(remote: &RemoteSession, path: &Path) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.create_file(&to_connection(remote), path)
}

pub fn create_dir_all(remote: &RemoteSession, path: &Path) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.create_dir_all(&to_connection(remote), path)
}

pub fn rename(remote: &RemoteSession, old: &Path, new: &Path) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.rename(&to_connection(remote), old, new)
}

pub fn remove_path(remote: &RemoteSession, path: &Path) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.remove_path(&to_connection(remote), path)
}

pub fn copy_recursive(remote: &RemoteSession, src: &Path, dst: &Path) -> std::io::Result<()> {
    let provider = SshRemoteFsProvider;
    provider.copy_recursive(&to_connection(remote), src, dst)
}

pub fn is_dir(remote: &RemoteSession, path: &Path) -> std::io::Result<bool> {
    let provider = SshRemoteFsProvider;
    provider.is_dir(&to_connection(remote), path)
}

pub fn global_search(
    remote: &RemoteSession,
    root: &Path,
    query: &str,
) -> (Vec<PathBuf>, HashMap<PathBuf, FileMetadata>) {
    let pattern = escape_shell_single_quoted(&format!("*{}*", query));
    let root_q = shell_quote_path(root);
    let cmd = format!("find {root_q} -type f -iname {pattern} 2>/dev/null | head -n 200");
    let out = run_command(remote, &cmd).unwrap_or_default();
    let files = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    (files, HashMap::new())
}

pub fn fetch_git_data(remote: &RemoteSession, path: &Path) -> Option<GitData> {
    let base = format!("cd {} && ", shell_quote_path(path));
    let branch = run_command(remote, &(base.clone() + "git rev-parse --abbrev-ref HEAD"))
        .ok()?
        .trim()
        .to_string();

    let (ahead, behind) = if let Ok(raw) = run_command(
        remote,
        &(base.clone() + "git rev-list --left-right --count HEAD...@{u}"),
    ) {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        if parts.len() == 2 {
            (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0))
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    let mut history = Vec::new();
    if let Ok(raw) = run_command(
        remote,
        &(base.clone()
            + "git --no-pager log -n 100 --pretty=format:%H%x1f%an%x1f%ar%x1f%s%x1f%d --shortstat"),
    ) {
        let mut current: Option<CommitInfo> = None;
        for line in raw.lines() {
            if let Some(parsed) = parse_git_log_record(line) {
                if let Some(c) = current.take() {
                    history.push(c);
                }
                current = Some(parsed);
            } else if let Some(c) = current.as_mut() {
                if line.contains("changed") {
                    let (files_changed, insertions, deletions) = parse_git_shortstat(line);
                    c.files_changed = files_changed;
                    c.insertions = insertions;
                    c.deletions = deletions;
                }
            }
        }
        if let Some(c) = current {
            history.push(c);
        }
    }

    let mut pending = Vec::new();
    let mut stats_map: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(raw) = run_command(remote, &(base.clone() + "git diff --numstat")) {
        for line in raw.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                stats_map.insert(
                    parts[2].to_string(),
                    (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0)),
                );
            }
        }
    }
    if let Ok(raw) = run_command(remote, &(base.clone() + "git diff --staged --numstat")) {
        for line in raw.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let entry = stats_map.entry(parts[2].to_string()).or_insert((0, 0));
                entry.0 += parts[0].parse::<usize>().unwrap_or(0);
                entry.1 += parts[1].parse::<usize>().unwrap_or(0);
            }
        }
    }
    if let Ok(raw) = run_command(remote, &(base.clone() + "git status --porcelain")) {
        for line in raw.lines() {
            if line.len() > 3 {
                let status = line[0..2].trim().to_string();
                let file = line[3..].to_string();
                let (ins, del) = stats_map.get(&file).cloned().unwrap_or((0, 0));
                pending.push(GitPendingChange {
                    status,
                    path: file,
                    insertions: ins,
                    deletions: del,
                });
            }
        }
    }

    let summary = run_command(remote, &(base.clone() + "git diff HEAD --shortstat"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let remotes = run_command(remote, &(base.clone() + "git remote -v"))
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let stashes = run_command(remote, &(base + "git stash list"))
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    Some((
        history, pending, branch, ahead, behind, summary, remotes, stashes,
    ))
}

pub fn build_remote_terminal_command(
    remote: &RemoteSession,
    path: &Path,
    command: Option<&str>,
) -> String {
    let dest = format!("{}@{}", remote.user, remote.host);
    let path_q = shell_quote_path(path);
    let body = if let Some(cmd) = command {
        format!("cd {path_q} && {cmd}")
    } else {
        format!("cd {path_q} && exec $SHELL -l")
    };
    let body_q = escape_shell_single_quoted(&body);
    let key_part = remote
        .key_path
        .as_ref()
        .map(|p| format!(" -i {}", shell_quote_path(p)))
        .unwrap_or_default();
    format!("ssh -p {}{} {} {}", remote.port, key_part, dest, body_q)
}

pub fn show_commit_patch(
    remote: &RemoteSession,
    repo_path: &Path,
    hash: &str,
) -> std::io::Result<String> {
    let cmd = format!(
        "cd {} && git show --patch --stat --color=never {}",
        shell_quote_path(repo_path),
        escape_shell_single_quoted(hash)
    );
    run_command(remote, &cmd)
}

pub fn show_file_diff(
    remote: &RemoteSession,
    repo_path: &Path,
    file_path: &str,
) -> std::io::Result<String> {
    let cmd = format!(
        "cd {} && git diff {}",
        shell_quote_path(repo_path),
        escape_shell_single_quoted(file_path)
    );
    let out = run_command(remote, &cmd)?;
    if out.trim().is_empty() {
        Ok("(No changes or file only in index)".to_string())
    } else {
        Ok(out)
    }
}

fn to_connection(remote: &RemoteSession) -> RemoteConnection {
    RemoteConnection {
        name: remote.name.clone(),
        host: remote.host.clone(),
        user: remote.user.clone(),
        port: remote.port,
        key_path: remote.key_path.clone(),
        auth_method: "session".to_string(),
    }
}

fn map_metadata(meta: RemoteEntryMetadata) -> FileMetadata {
    FileMetadata {
        size: meta.size,
        modified: meta.modified,
        created: meta.created,
        permissions: meta.permissions,
        is_dir: meta.is_dir,
    }
}

fn run_command(remote: &RemoteSession, command: &str) -> std::io::Result<String> {
    let exec = SshRemoteExecProvider;
    exec.run_command(&to_connection(remote), command)
}

fn parse_git_log_record(line: &str) -> Option<CommitInfo> {
    let parts: Vec<&str> = line.trim().split('\x1f').collect();
    if parts.len() < 5 {
        return None;
    }
    Some(CommitInfo {
        hash: parts[0].to_string(),
        author: parts[1].to_string(),
        date: parts[2].to_string(),
        message: parts[3].to_string(),
        decorations: parts[4].to_string(),
        files_changed: 0,
        insertions: 0,
        deletions: 0,
    })
}

fn parse_git_shortstat(line: &str) -> (usize, usize, usize) {
    let mut files_changed = 0usize;
    let mut insertions = 0usize;
    let mut deletions = 0usize;
    for segment in line.split(',').map(str::trim) {
        let value = segment
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        if segment.contains("file changed") || segment.contains("files changed") {
            files_changed = value;
        } else if segment.contains("insertion") {
            insertions = value;
        } else if segment.contains("deletion") {
            deletions = value;
        }
    }
    (files_changed, insertions, deletions)
}

fn shell_quote_path(path: &Path) -> String {
    escape_shell_single_quoted(&path.to_string_lossy())
}

fn escape_shell_single_quoted(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_remote() -> RemoteSession {
        RemoteSession {
            host: "example.com".to_string(),
            user: "dracon".to_string(),
            name: "test".to_string(),
            port: 2222,
            key_path: Some(PathBuf::from("/home/dracon/.ssh/id_ed25519")),
        }
    }

    #[test]
    fn escape_single_quotes_for_shell() {
        let escaped = escape_shell_single_quoted("a'b");
        assert_eq!(escaped, "'a'\"'\"'b'");
    }

    #[test]
    fn build_remote_terminal_command_includes_key_and_port() {
        let remote = sample_remote();
        let cmd = build_remote_terminal_command(&remote, Path::new("/tmp/my dir"), Some("ls -la"));
        assert!(cmd.starts_with("ssh -p 2222 "));
        assert!(cmd.contains(" -i '/home/dracon/.ssh/id_ed25519' "));
        assert!(cmd.contains(" dracon@example.com "));
        assert!(cmd.contains("/tmp/my dir"));
        assert!(cmd.contains("ls -la"));
    }

    #[test]
    fn build_remote_terminal_command_defaults_to_shell_login() {
        let remote = RemoteSession {
            key_path: None,
            ..sample_remote()
        };
        let cmd = build_remote_terminal_command(&remote, Path::new("/"), None);
        assert!(cmd.contains("ssh -p 2222 "));
        assert!(cmd.contains(" dracon@example.com "));
        assert!(cmd.contains("exec $SHELL -l"));
    }

    #[test]
    fn parse_git_log_record_reads_all_fields() {
        let line = "abc123\x1fAda\x1f2 days ago\x1ffeat: add parser\x1f(HEAD -> main)";
        let parsed = parse_git_log_record(line).expect("record should parse");
        assert_eq!(parsed.hash, "abc123");
        assert_eq!(parsed.author, "Ada");
        assert_eq!(parsed.date, "2 days ago");
        assert_eq!(parsed.message, "feat: add parser");
        assert_eq!(parsed.decorations, "(HEAD -> main)");
    }

    #[test]
    fn parse_git_shortstat_extracts_counts() {
        let (files, ins, del) =
            parse_git_shortstat("3 files changed, 21 insertions(+), 8 deletions(-)");
        assert_eq!(files, 3);
        assert_eq!(ins, 21);
        assert_eq!(del, 8);
    }
}
