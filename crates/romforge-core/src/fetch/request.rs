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
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub url: &'a str,
    pub headers: Vec<(&'a str, String)>,
    /// `Some` makes it a POST with this body.
    pub body: Option<String>,
}

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Self {
        Request {
            url,
            ..Request::default()
        }
    }
}

/// The URL without its query (which may be long, or hold credentials).
fn short(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Send a request; `Ok(None)` for 404. A busy server (429, 503) is asked
/// again after the wait it gives in Retry-After, twice at most; one that
/// asks for more than 30 s is not, and the error says how long it wants.
pub fn request(req: &Request) -> Result<Option<Vec<u8>>, Error> {
    let Some(mut response) = respond(req)? else {
        return Ok(None);
    };
    response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map(Some)
        .map_err(|e| Error::Http(format!("{}: {e}", short(req.url))))
}

/// GET `url` straight into the file `to` (written as `<to>.part`, renamed
/// when complete), for downloads too big to hold in memory. `progress`
/// gets (bytes so far, total if the server said). Returns the size.
pub fn download_to(
    url: &str,
    to: &std::path::Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64, Error> {
    let err = |m: String| Error::Http(format!("{}: {m}", short(url)));
    let mut response = respond(&Request::get(url))?.ok_or_else(|| err("HTTP 404".into()))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    let part = to.with_extension(format!(
        "{}.part",
        to.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
    let mut body = response.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err(err(e.to_string()));
            }
        };
        std::io::Write::write_all(&mut out, &buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    std::io::Write::flush(&mut out)?;
    drop(out);
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
    let agent = agent();
    for attempt in 0.. {
        wait_for_host(url);
        let result = match &req.body {
            Some(body) => {
                let mut r = agent.post(url);
                for (k, v) in &req.headers {
                    r = r.header(*k, v);
                }
                r.send(body.as_str())
            }
            None => {
                let mut r = agent.get(url);
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
                return Err(err(format!(
                    "HTTP {status}: the server is rate limiting; it asks to wait {} before trying again",
                    if wait > 90 {
                        format!("{minutes} minutes")
                    } else {
                        format!("{wait} s")
                    }
                )));
            }
            std::thread::sleep(Duration::from_secs(wait));
            continue;
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
    e.to_string().contains("rate limiting")
}

/// Percent-encode one URL path segment.
pub fn encode_segment(s: &str) -> String {
    percent_encode(s)
}

fn wait_for_host(url: &str) {
    static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
    let host = url.split('/').nth(2).unwrap_or(url).to_owned();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = last.get_or_insert_with(HashMap::new);
    if let Some(t) = last.get(&host) {
        let (since, delay) = (t.elapsed(), delay_for(&host));
        if since < delay {
            std::thread::sleep(delay - since);
        }
    }
    last.insert(host, Instant::now());
}
