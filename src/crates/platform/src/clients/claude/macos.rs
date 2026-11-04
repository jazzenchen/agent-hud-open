pub(crate) const PROJECTS_SUPPORTED: bool = true;

pub(crate) fn open_project(url: &str) -> Result<bool, String> {
    // Target Claude explicitly: a generic URL handler can lose the thread query.
    std::process::Command::new("/usr/bin/open")
        .args(["-b", "com.anthropic.claudefordesktop", url])
        .output()
        .map(|output| output.status.success())
        .map_err(|_| "Claude Desktop could not be opened".into())
}
