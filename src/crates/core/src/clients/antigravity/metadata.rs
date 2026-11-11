//! A conversation's metadata, the blob its database keeps: the conversation
//! it was started from, its role, agent name, folder and creation time.
use agent_hud_agents::antigravity::proto as antigravity_proto;
use agent_hud_agents::transcript::sql_tables;
use rusqlite::Connection;

pub(super) fn trajectory_metadata(client: &Connection) -> antigravity_proto::TrajectoryMetadata {
    let Ok(tables) = sql_tables(client) else {
        return Default::default();
    };
    if !tables.contains("trajectory_metadata_blob") {
        return Default::default();
    }
    client
        .query_row(
            "SELECT data FROM trajectory_metadata_blob LIMIT 1",
            [],
            |row| match row.get_ref(0)? {
                rusqlite::types::ValueRef::Blob(b) if b.len() <= 1024 * 1024 => {
                    Ok(antigravity_proto::metadata(b).unwrap_or_default())
                }
                _ => Ok(Default::default()),
            },
        )
        .unwrap_or_default()
}
