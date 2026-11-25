use super::*;

pub(super) const CODEX_WITHOUT_DESKTOP: Option<&str> = Some("cli");

// The Unix start route does not read a project catalog.
pub(super) fn mock_empty_codex_projects(_fixture: &StartFixture, _starts: &mut SessionStarts) {}
