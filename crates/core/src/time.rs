use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Error, Result};

pub(crate) fn unix_millis() -> Result<i64> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::InvalidTime)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::InvalidTime)
}
