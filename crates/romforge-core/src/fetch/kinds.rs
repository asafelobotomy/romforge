//! What a failed request says about its server.

use super::*;

/// Whether an error from [`request`] is a server asking to be left alone.
pub fn is_rate_limited(e: &Error) -> bool {
    matches!(e, Error::RateLimited(_))
}

/// Whether a source should be asked nothing more this run: it is rate
/// limiting, or it refused the key it was given.
pub fn stops_source(e: &Error) -> bool {
    matches!(e, Error::RateLimited(_) | Error::Refused(_))
}

/// Whether an error from [`request`] is a server down, hanging or failing
/// (no connection, no answer in time, HTTP 5xx).
pub fn is_unreachable(e: &Error) -> bool {
    matches!(e, Error::Unreachable(_))
}
