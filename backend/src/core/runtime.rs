//! Desktop overrides; server builds retain their executable-relative layout.
use std::{path::PathBuf, sync::OnceLock};
static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();
static TOOL_ROOT: OnceLock<PathBuf> = OnceLock::new();
pub fn initialize(data: PathBuf, tools: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&data)?;
    DATA_ROOT.set(data).map_err(|_| std::io::Error::other("Runtime already initialized"))?;
    TOOL_ROOT.set(tools).map_err(|_| std::io::Error::other("Tools already initialized"))?;
    Ok(())
}
pub fn data_root() -> Option<PathBuf> { DATA_ROOT.get().cloned() }
pub fn tool(name: &str) -> PathBuf {
    TOOL_ROOT.get().map(|root| root.join(name)).unwrap_or_else(|| PathBuf::from(name))
}
pub fn child_path() -> std::ffi::OsString {
    let mut paths = Vec::new();
    if let Some(root) = TOOL_ROOT.get() { paths.push(root.clone()); }
    if let Some(path) = std::env::var_os("PATH") { paths.extend(std::env::split_paths(&path)); }
    std::env::join_paths(paths).unwrap_or_default()
}
