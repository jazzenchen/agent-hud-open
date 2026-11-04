use super::{Credentials, Failure};
use std::path::Path;

pub(super) fn credentials(_home: &Path) -> Result<Credentials, Failure> {
    Err(Failure::SignedOut)
}
