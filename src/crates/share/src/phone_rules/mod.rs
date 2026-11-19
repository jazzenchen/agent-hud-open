//! The phone's checks on each record of a local network reading, so a record
//! it would refuse stays out of the reading.
//!
//! The phone refuses a whole reading for one record it finds wrong and shows
//! "The Mac's update did not match this connection", where over iCloud it only
//! leaves that record out. These follow the phone's `LocalNetworkProjection`
//! and `SyncProjection.isValid` (iPhone 1.7.10) for the kinds written here: the
//! fields its Swift types need, then the rules it adds.
mod fields;
mod readings;
mod usage;

pub use fields::{decimal, ensure, Check, Object};

use crate::record::{version, SyncRecord};

/// Why a phone would refuse `record` in a reading, or None. Kinds this crate
/// does not write are refused: a product that writes more checks those itself
/// and hands the rest here.
pub fn refusal(record: &SyncRecord) -> Option<String> {
    let check = envelope(record).and_then(|()| match record.kind.as_str() {
        "device" => device(record),
        "consumer" => consumer(record),
        "usage" => usage::check(record),
        "readings" => readings::check(record),
        other => Err(format!("phones do not read {other} records here")),
    });
    refused(record, check)
}

/// `check` as `refusal` says it: None when it passed, otherwise the reason
/// with the record it is about.
pub fn refused(record: &SyncRecord, check: Check) -> Option<String> {
    check
        .err()
        .map(|reason| format!("{} {}: {reason}", record.kind, record.key))
}

/// What the phone asks of every record before its kind's rules: it belongs to
/// a device and is in its kind's version.
pub fn envelope(record: &SyncRecord) -> Check {
    ensure(!record.device_id.is_empty(), || "no device".into())?;
    ensure(record.schema_version == version(&record.kind), || {
        format!("version {} is not its kind's", record.schema_version)
    })
}

/// The payload as an object, which every kind's is.
pub fn payload(record: &SyncRecord) -> Result<Object<'_>, String> {
    Object::of(&record.payload, "payload")
}

fn device(record: &SyncRecord) -> Check {
    let value = payload(record)?;
    let name = value.text("name")?;
    let heartbeat = value.int("heartbeatAtMs")?;
    if let Some(connection) = value.object_opt("localConnection", "localConnection")? {
        connection.text("endpoint")?;
        connection.text("certificateSHA256")?;
        connection.text("token")?;
        connection.texts_opt("alternateEndpoints")?;
    }
    if let Some(cursor) = value.object_opt("businessVersion", "businessVersion")? {
        cursor.text("baseVersion")?;
        cursor.int("changeSequence")?;
    }
    for offer in value.objects_opt("starts", "start")?.unwrap_or_default() {
        offer.text("provider")?;
        offer.text("app")?;
        offer.texts("folders")?;
        offer.flag_opt("withoutFolder")?;
    }
    if let Some(app) = value.object_opt("app", "app")? {
        app_version(app)?;
    }
    if let Some(recommends) = value.object_opt("recommends", "recommends")? {
        for device in ["iphone", "mac", "watch"] {
            if let Some(app) = recommends.object_opt(device, device)? {
                app_version(app)?;
            }
        }
    }
    value.text_opt("platform")?;
    ensure(record.key == record.device_id, || {
        "key is not its device".into()
    })?;
    ensure(!name.is_empty(), || "no name".into())?;
    ensure(heartbeat > 0, || "no heartbeat".into())
}

fn app_version(app: Object) -> Check {
    app.text("version")?;
    app.int("build").map(drop)
}

fn consumer(record: &SyncRecord) -> Check {
    let value = payload(record)?;
    value.text("vendor")?;
    value.text("model").map(drop)
}

#[cfg(test)]
mod tests;
