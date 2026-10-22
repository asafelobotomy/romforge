//! Small answers: an API's JSON, a SPARQL result, a listing.

use super::*;

/// The most a small JSON or SPARQL answer may be.
pub const MAX_SMALL: u64 = 32 << 20;
/// A small answer's deadline, body included (Wikidata's query service
/// gives up on a query after 60 s itself).
pub const SMALL_TIMEOUT: Duration = Duration::from_secs(90);

impl<'a> Request<'a> {
    /// A GET for a small answer (an API's JSON, a SPARQL result, a
    /// listing): [`MAX_SMALL`] and [`SMALL_TIMEOUT`].
    pub fn small(url: &'a str) -> Self {
        Request {
            max_body: Some(MAX_SMALL),
            timeout: Some(SMALL_TIMEOUT),
            ..Request::get(url)
        }
    }
}

/// [`get`] for a small answer ([`Request::small`]'s limits).
pub fn get_small(url: &str) -> Result<Vec<u8>, Error> {
    request(&Request::small(url))?.ok_or_else(|| Error::Http(format!("{}: HTTP 404", short(url))))
}

/// [`get_optional`] for a small answer ([`Request::small`]'s limits).
pub fn get_small_optional(url: &str) -> Result<Option<Vec<u8>>, Error> {
    request(&Request::small(url))
}
