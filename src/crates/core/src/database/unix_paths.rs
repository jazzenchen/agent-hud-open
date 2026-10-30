//! Unix paths need no Windows separator migration.
use rusqlite::Connection;

pub(super) fn migrate(_db: &Connection) -> rusqlite::Result<()> {
    Ok(())
}
