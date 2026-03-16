pub mod add;
pub mod check;
pub mod cspell;
pub mod dict;
pub mod init;
pub mod review;
pub mod setup;
pub mod trace;

use std::path::PathBuf;

/// Central cache directory for downloaded dictionary packages.
pub fn dict_cache_dir() -> PathBuf {
    matchum_config::npm_fetch::default_cache_dir()
}
