//! Client-specific work that still reads or writes what core owns: Antigravity's
//! reader stages rows in the ledger, Claude's desktop questions live on
//! AgentHudCore, and WorkBuddy's readings walk core's log heads. Each moves to
//! its client's folder in `agents` once those seams have an interface
//! (docs/agent-contract.md).
pub(crate) mod antigravity;
pub(crate) mod claude_cloud;
pub(crate) mod claude_desktop;
pub(crate) mod claude_project;
pub(crate) mod workbuddy;
