/// Credentials for the online sources that need them. All optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Keys {
    /// ScreenScraper developer credentials (issued to an app).
    pub screenscraper_dev_id: Option<String>,
    pub screenscraper_dev_password: Option<String>,
    /// ScreenScraper user account (optional; raises the request limits).
    pub screenscraper_user: Option<String>,
    pub screenscraper_password: Option<String>,
    pub steamgriddb_key: Option<String>,
    /// IGDB goes through a Twitch application.
    pub igdb_client_id: Option<String>,
    pub igdb_client_secret: Option<String>,
}

/// What a set secret looks like outside the core (the app's settings
/// form); saving it back keeps the stored value.
pub const MASK: &str = "••••••••";

impl Keys {
    /// The secrets (passwords, keys) masked; ids and user names as they are.
    pub fn masked(&self) -> Keys {
        let m = |v: &Option<String>| v.as_ref().map(|_| MASK.to_owned());
        Keys {
            screenscraper_dev_password: m(&self.screenscraper_dev_password),
            screenscraper_password: m(&self.screenscraper_password),
            steamgriddb_key: m(&self.steamgriddb_key),
            igdb_client_secret: m(&self.igdb_client_secret),
            ..self.clone()
        }
    }

    /// `self` from a form that got [`Keys::masked`]: a field still showing
    /// the mask keeps the `stored` value.
    pub fn unmasked(self, stored: &Keys) -> Keys {
        let keep = |v: Option<String>, old: &Option<String>| {
            if v.as_deref() == Some(MASK) {
                old.clone()
            } else {
                v
            }
        };
        Keys {
            screenscraper_dev_password: keep(
                self.screenscraper_dev_password,
                &stored.screenscraper_dev_password,
            ),
            screenscraper_password: keep(self.screenscraper_password, &stored.screenscraper_password),
            steamgriddb_key: keep(self.steamgriddb_key, &stored.steamgriddb_key),
            igdb_client_secret: keep(self.igdb_client_secret, &stored.igdb_client_secret),
            ..self
        }
    }

    /// (setting key, value) pairs, in a fixed order.
    pub(super) fn fields(&self) -> [(&'static str, Option<String>); 7] {
        [
            ("screenscraper_dev_id", self.screenscraper_dev_id.clone()),
            (
                "screenscraper_dev_password",
                self.screenscraper_dev_password.clone(),
            ),
            ("screenscraper_user", self.screenscraper_user.clone()),
            ("screenscraper_password", self.screenscraper_password.clone()),
            ("steamgriddb_key", self.steamgriddb_key.clone()),
            ("igdb_client_id", self.igdb_client_id.clone()),
            ("igdb_client_secret", self.igdb_client_secret.clone()),
        ]
    }

    pub(super) fn trimmed(&self) -> Keys {
        let t = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        Keys {
            screenscraper_dev_id: t(&self.screenscraper_dev_id),
            screenscraper_dev_password: t(&self.screenscraper_dev_password),
            screenscraper_user: t(&self.screenscraper_user),
            screenscraper_password: t(&self.screenscraper_password),
            steamgriddb_key: t(&self.steamgriddb_key),
            igdb_client_id: t(&self.igdb_client_id),
            igdb_client_secret: t(&self.igdb_client_secret),
        }
    }

    pub fn screenscraper(&self) -> bool {
        self.screenscraper_dev_id.is_some() && self.screenscraper_dev_password.is_some()
    }

    pub fn steamgriddb(&self) -> bool {
        self.steamgriddb_key.is_some()
    }

    pub fn igdb(&self) -> bool {
        self.igdb_client_id.is_some() && self.igdb_client_secret.is_some()
    }
}

/// `abc…xyz` for display: enough to recognize a key, not to use it.
pub fn mask(secret: &str) -> String {
    let n = secret.chars().count();
    if n <= 8 {
        "•".repeat(n.max(4))
    } else {
        let head: String = secret.chars().take(3).collect();
        let tail: String = secret.chars().skip(n - 3).collect();
        format!("{head}…{tail}")
    }
}
