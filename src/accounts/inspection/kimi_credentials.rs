//! Select the native credential and its managed endpoint as one observation.
use std::{
    env, fs,
    io::{self, Read},
    path::Path,
};

const CONFIG_LIMIT: u64 = 256 * 1024;
const GRANT_LIMIT: u64 = super::MAX_OUTPUT;
const CONFIG_ERROR: &str = "The Kimi account configuration is unreadable or too large.";
const CONFIG_ENDPOINT: &str = "The Kimi account does not name an official managed endpoint.";
const GRANT_MISMATCH: &str =
    "The managed Kimi endpoint and its configured OAuth credential do not match.";
const GRANT_ERROR: &str = "The managed Kimi credential file is unreadable or too large.";
pub(super) const API_ENV: &str = "KIMI_CODE_BASE_URL";
pub(super) const OAUTH_ENV: &str = "KIMI_CODE_OAUTH_HOST";
pub(super) const LEGACY_OAUTH_ENV: &str = "KIMI_OAUTH_HOST";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Region {
    Mainland,
    Global,
}

impl Region {
    const ALL: [Self; 2] = [Self::Mainland, Self::Global];

    pub(super) fn api(self) -> &'static str {
        match self {
            Self::Mainland => "https://api.kimi.com/coding/v1",
            Self::Global => "https://api.kimi.ai/coding/v1",
        }
    }

    pub(super) fn oauth(self) -> &'static str {
        match self {
            Self::Mainland => "https://auth.kimi.com",
            Self::Global => "https://auth.kimi.ai",
        }
    }

    fn slot(self) -> &'static str {
        // Kimi derives a scoped slot from SHA-256 of its normalized OAuth
        // host and API base. Only the default official pair uses the old slot.
        match self {
            Self::Mainland => "kimi-code",
            Self::Global => "kimi-code-env-0e4f99c69cc27850",
        }
    }
}

pub(super) struct Grant {
    pub region: Region,
    pub bytes: Option<Vec<u8>>,
    pub test_transport_only: bool,
}

/// Only a missing file is an absent value. Refuse long or non-regular files
/// before parsing, and cap the actual read even if a file grows after opening.
fn bounded(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut options = fs::OpenOptions::new();
    let opened = crate::platform::no_follow(options.read(true)).open(path);
    let file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(Some(bytes))
}

fn configured_region(home: &Path) -> Result<Option<Region>, &'static str> {
    let Some(bytes) = bounded(&home.join("config.toml"), CONFIG_LIMIT).map_err(|_| CONFIG_ERROR)?
    else {
        return Ok(None);
    };
    let document = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
        .ok_or(CONFIG_ERROR)?;
    let Some(providers) = document.get("providers") else {
        return Ok(None);
    };
    let Some(provider) = providers
        .as_table_like()
        .ok_or(CONFIG_ERROR)?
        .get("managed:kimi-code")
    else {
        return Ok(None);
    };
    let provider = provider.as_table_like().ok_or(CONFIG_ERROR)?;
    let base = provider
        .get("base_url")
        .and_then(toml_edit::Item::as_str)
        .ok_or(CONFIG_ENDPOINT)?;
    let region = Region::ALL
        .into_iter()
        .find(|region| region.api() == base.trim_end_matches('/'))
        .ok_or(CONFIG_ENDPOINT)?;
    let oauth = provider
        .get("oauth")
        .and_then(toml_edit::Item::as_table_like)
        .ok_or(GRANT_MISMATCH)?;
    let key = oauth
        .get("key")
        .and_then(toml_edit::Item::as_str)
        .and_then(|key| key.strip_prefix("oauth/"));
    let host_matches = oauth
        .get("oauth_host")
        .map_or(region == Region::Mainland, |host| {
            host.as_str()
                .is_some_and(|host| host.trim_end_matches('/') == region.oauth())
        });
    if key != Some(region.slot())
        || !host_matches
        || oauth
            .get("storage")
            .is_some_and(|value| value.as_str() != Some("file"))
    {
        return Err(GRANT_MISMATCH);
    }
    Ok(Some(region))
}

fn same_test_origin(base: &str, test_base: &str) -> bool {
    fn origin(url: &str) -> Option<(&str, &str)> {
        let (scheme, rest) = url.split_once("://")?;
        Some((scheme, rest.split('/').next()?))
    }
    super::loopback_test_base(base).is_some() && origin(base) == origin(test_base)
}

pub(super) fn load(home: &Path, test_base: Option<&str>) -> Result<Grant, &'static str> {
    let configured = configured_region(home)?;
    let mut test_transport_only = false;
    let selected = match env::var(API_ENV) {
        Ok(base) => match Region::ALL
            .into_iter()
            .find(|region| region.api() == base.strip_suffix('/').unwrap_or(&base))
        {
            Some(region) if configured.is_none_or(|configured| configured == region) => Some(region),
            None if configured.is_none() && test_base.is_some_and(|test| same_test_origin(&base, test)) => {
                // The offline Teams fixture runs the native CLI against the
                // same loopback origin. This exception cannot start a refresh
                // child or forward a grant to the native CLI's endpoint.
                test_transport_only = true;
                None
            }
            _ => return Err("Ignored KIMI_CODE_BASE_URL: it conflicts with the managed Kimi account or is not an official endpoint."),
        },
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => return Err("Ignored KIMI_CODE_BASE_URL: the value is unreadable."),
    };
    let region = selected.or(configured).unwrap_or(Region::Mainland);
    let oauth = match env::var(OAUTH_ENV) {
        Err(env::VarError::NotPresent) => env::var(LEGACY_OAUTH_ENV),
        value => value,
    };
    match oauth {
        Ok(host) if host.trim_end_matches('/') == region.oauth() => (),
        Err(env::VarError::NotPresent) => (),
        _ => {
            return Err(
                "The Kimi OAuth host override conflicts with the selected managed endpoint.",
            )
        }
    }

    let folder = home.join("credentials");
    match fs::symlink_metadata(&folder) {
        Ok(metadata) if metadata.is_dir() => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        _ => return Err(GRANT_ERROR),
    }
    let path = folder.join(format!("{}.json", region.slot()));
    let bytes = bounded(&path, GRANT_LIMIT).map_err(|_| GRANT_ERROR)?;
    Ok(Grant {
        region,
        bytes,
        test_transport_only,
    })
}
