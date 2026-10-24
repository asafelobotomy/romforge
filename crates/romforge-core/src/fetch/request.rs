use super::*;

/// Minimum time between two requests to `host`.
pub fn delay_for(host: &str) -> Duration {
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    match name {
        "datomatic.no-intro.org" => Duration::from_secs(5),
        "127.0.0.1" | "localhost" | "[::1]" => Duration::ZERO,
        // Wikimedia asks bots to keep requests serial and slow.
        "query.wikidata.org" | "www.wikidata.org" => Duration::from_secs(2),
        _ => Duration::from_secs(1),
    }
}

/// [`get`], but a 404 is `None` rather than an error.
pub fn get_optional(url: &str) -> Result<Option<Vec<u8>>, Error> {
    request(&Request::get(url))
}

/// GET `url` into memory, waiting first if the same host was asked less than
/// [`delay_for`] ago.
pub fn get(url: &str) -> Result<Vec<u8>, Error> {
    request(&Request::get(url))?.ok_or_else(|| Error::Http(format!("{}: HTTP 404", short(url))))
}

/// An HTTP request beyond a plain GET (API keys in headers, POSTed queries).
#[derive(Clone, Default)]
pub struct Request<'a> {
    pub url: &'a str,
    pub headers: Vec<(&'a str, String)>,
    /// `Some` makes it a POST with this body.
    pub body: Option<String>,
    /// The most the body may be; `None`: a DAT pack's limit.
    pub max_body: Option<u64>,
    /// A deadline for the whole request, body included; `None`: the
    /// agent's (a slow link may take hours over a DAT pack).
    pub timeout: Option<Duration>,
}

/// Header values and the query (which may hold keys) never show.
impl std::fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, &str)> = self.headers.iter().map(|(k, _)| (*k, "<redacted>")).collect();
        f.debug_struct("Request")
            .field("url", &short(self.url))
            .field("headers", &headers)
            .field(
                "body",
                &self.body.as_ref().map(|b| format!("<{} bytes>", b.len())),
            )
            .field("max_body", &self.max_body)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// The most a picture may be: no cover or fanart is near this.
pub const MAX_IMAGE: u64 = 32 << 20;
/// A picture's deadline, body included.
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(90);

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Self {
        Request {
            url,
            ..Request::default()
        }
    }

    /// A GET for a picture: [`MAX_IMAGE`] and [`IMAGE_TIMEOUT`].
    pub fn image(url: &'a str) -> Self {
        Request {
            max_body: Some(MAX_IMAGE),
            timeout: Some(IMAGE_TIMEOUT),
            ..Request::get(url)
        }
    }
}

/// [`get_optional`] for a picture ([`Request::image`]'s limits).
pub fn get_image(url: &str) -> Result<Option<Vec<u8>>, Error> {
    request(&Request::image(url))
}

/// The URL without its query (which may be long, or hold credentials).
pub(crate) fn short(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// The host (and port) of a URL: "a.org:8080" of "https://a.org:8080/x?y".
pub(crate) fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// Send a request; `Ok(None)` for 404. A busy server (429, 503) is asked
/// again after the wait it gives in Retry-After, twice at most; one that
/// asks for more than 30 s is not, and the error says how long it wants.
/// A used-up quota (ScreenScraper's 430 and 431) is not asked again.
pub fn request(req: &Request) -> Result<Option<Vec<u8>>, Error> {
    let _one = one_at_a_time(req.url);
    let Some(mut response) = respond(req)? else {
        return Ok(None);
    };
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    let mut body = response
        .body_mut()
        .with_config()
        .limit(req.max_body.unwrap_or(MAX_BODY))
        .reader();
    // Read in pieces so a watched download shows how far it is
    // ([`super::watching`]) and can be stopped.
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = body
            .read(&mut buf)
            .map_err(|e| Error::Http(format!("{}: {e}", short(req.url))))?;
        if n == 0 {
            return Ok(Some(out));
        }
        out.extend_from_slice(&buf[..n]);
        if !super::watch::report(out.len() as u64, total) {
            return Err(Error::Cancelled);
        }
    }
}

/// GET `url` straight into the file `to` (written as `<to>.part`, renamed
/// when complete), for downloads too big to hold in memory, `max` bytes at
/// most. `progress` gets (bytes so far, total if the server said). Returns
/// the size.
pub fn download_to(
    url: &str,
    to: &std::path::Path,
    max: u64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64, Error> {
    let err = |m: String| Error::Http(format!("{}: {m}", short(url)));
    let _one = one_at_a_time(url);
    let mut response = respond(&Request::get(url))?.ok_or_else(|| err("HTTP 404".into()))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    if let Some(t) = total.filter(|t| *t > max) {
        return Err(err(format!("{t} bytes, more than the {max} allowed")));
    }
    let part = to.with_extension(format!(
        "{}.part",
        to.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::io::BufWriter::new(std::fs::File::create(&part)?);
    let body = response.body_mut().as_reader();
    let done = match write_body(body, out, max, total, &mut progress, &err) {
        Ok(done) => done,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    if total.is_some_and(|t| t != done) {
        let _ = std::fs::remove_file(&part);
        return Err(err(format!(
            "download cut short ({done} of {} bytes)",
            total.unwrap_or(0)
        )));
    }
    std::fs::rename(&part, to)?;
    Ok(done)
}

/// [`download_to`]'s copy from `body` into `out`; the caller removes the
/// part file when this fails.
fn write_body(
    mut body: impl Read,
    mut out: impl std::io::Write,
    max: u64,
    total: Option<u64>,
    progress: &mut impl FnMut(u64, Option<u64>),
    err: &impl Fn(String) -> Error,
) -> Result<u64, Error> {
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(err(e.to_string())),
        };
        done += n as u64;
        if done > max {
            return Err(err(format!("more than the {max} bytes allowed")));
        }
        out.write_all(&buf[..n])?;
        progress(done, total);
        if !super::watch::report(done, total) {
            return Err(Error::Cancelled);
        }
    }
    out.flush()?;
    Ok(done)
}

/// One HTTP agent for the whole process: building one loads the system's
/// certificates, which costs far more than a request.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            // Dead or silent servers fail fast; a body may take long on a
            // slow link (LaunchBox's 110 MB, TOSEC's pack), so it gets
            // hours rather than one deadline for the whole request.
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(120)))
            .timeout_recv_body(Some(Duration::from_secs(3 * 3600)))
            .http_status_as_error(false)
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent()
    })
}

/// Send a request, waiting out short rate limits; `Ok(None)` for 404.
fn respond(req: &Request) -> Result<Option<ureq::http::Response<ureq::Body>>, Error> {
    let url = req.url;
    let err = |m: String| Error::Http(format!("{}: {m}", short(url)));
    let limited = |m: String| Error::RateLimited(format!("{}: {m}", short(url)));
    let agent = agent();
    for attempt in 0.. {
        wait_for_host(url);
        let result = match &req.body {
            Some(body) => {
                let mut r = agent.post(url);
                if req.timeout.is_some() {
                    r = r.config().timeout_global(req.timeout).build();
                }
                for (k, v) in &req.headers {
                    r = r.header(*k, v);
                }
                r.send(body.as_str())
            }
            None => {
                let mut r = agent.get(url);
                if req.timeout.is_some() {
                    r = r.config().timeout_global(req.timeout).build();
                }
                for (k, v) in &req.headers {
                    r = r.header(*k, v);
                }
                r.call()
            }
        };
        let response = result.map_err(|e| err(e.to_string()))?;
        let status = response.status().as_u16();
        if status == 429 || status == 503 {
            let wait = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(5);
            if wait > 30 || attempt >= 2 {
                let minutes = wait.div_ceil(60);
                return Err(limited(format!(
                    "HTTP {status}: the server is rate limiting; it asks to wait {} before trying again",
                    if wait > 90 {
                        format!("{minutes} minutes")
                    } else {
                        format!("{wait} s")
                    }
                )));
            }
            // Every worker waits it out, not only this one.
            hold_host(url, Duration::from_secs(wait));
            continue;
        }
        // ScreenScraper's daily quota (430) and its limit on unknown ROMs
        // (431): nothing more today.
        if status == 430 || status == 431 {
            return Err(limited(format!(
                "HTTP {status}: the server is rate limiting; its quota for today is used up"
            )));
        }
        if status == 404 {
            return Ok(None);
        }
        if status >= 400 {
            return Err(err(format!("HTTP {status}")));
        }
        return Ok(Some(response));
    }
    unreachable!("the loop returns")
}

/// Whether an error from [`request`] is a server asking to be left alone.
pub fn is_rate_limited(e: &Error) -> bool {
    matches!(e, Error::RateLimited(_))
}

/// Percent-encode one URL path segment.
pub fn encode_segment(s: &str) -> String {
    percent_encode(s)
}

/// Undo percent-encoding ("Pok%C3%A9mon" -> "Pokémon"); a `%` not followed
/// by two hex digits stays as it is.
pub fn decode_segment(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (
                b.get(i + 1).and_then(|c| hex(*c)),
                b.get(i + 2).and_then(|c| hex(*c)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Hosts that ask for one connection at a time from each user (the Arcade
/// Database: "a single connection per ip address at a time").
const ONE_AT_A_TIME: &[&str] = &["adb.arcadeitalia.net"];

/// Held for a whole request (answer and body) to an [`ONE_AT_A_TIME`] host.
static ONE: Mutex<()> = Mutex::new(());

fn one_at_a_time(url: &str) -> Option<std::sync::MutexGuard<'static, ()>> {
    let host = host_of(url);
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    ONE_AT_A_TIME
        .contains(&name)
        .then(|| ONE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// When each host may be asked next.
static NEXT: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Ask `url`'s host nothing more for `wait` (it said so with a 429 or 503),
/// whichever worker asks next.
fn hold_host(url: &str, wait: Duration) {
    let host = host_of(url).to_owned();
    let mut next = NEXT.lock().unwrap_or_else(|e| e.into_inner());
    let next = next.get_or_insert_with(HashMap::new);
    let until = Instant::now() + wait;
    let at = next.entry(host).or_insert(until);
    *at = (*at).max(until);
}

/// Wait until `url`'s host may be asked again. The slot is taken under the
/// lock and waited for outside it, so one slow host never holds up others.
pub(super) fn wait_for_host(url: &str) {
    let host = host_of(url).to_owned();
    let at = {
        let mut next = NEXT.lock().unwrap_or_else(|e| e.into_inner());
        let next = next.get_or_insert_with(HashMap::new);
        let now = Instant::now();
        let at = next.get(&host).map_or(now, |t| (*t).max(now));
        next.insert(host.clone(), at + delay_for(&host));
        at
    };
    let now = Instant::now();
    if at > now {
        std::thread::sleep(at - now);
    }
}

#[cfg(test)]
mod one_tests {
    #[test]
    fn the_arcade_database_gets_one_request_at_a_time() {
        assert!(super::one_at_a_time("https://example.org/a").is_none());
        let held = super::one_at_a_time("https://adb.arcadeitalia.net/service_scraper.php?x").unwrap();
        assert!(super::ONE.try_lock().is_err(), "a second request waits");
        drop(held);
        assert!(super::ONE.try_lock().is_ok());
    }
}
