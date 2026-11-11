//! DeepSeek has no hooks: its questions come through a native observer. It is
//! known here so that callbacks naming it are refused by name.
use crate::hooks::{Hooks, Layout, POSIX_ONLY};

pub static HOOKS: Hooks = Hooks {
    settings: None,
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: None,
    completion: None,
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};
