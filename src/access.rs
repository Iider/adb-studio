use anyhow::{Result, ensure};
use axum::http::{HeaderMap, header};
use reqwest::Url;

pub struct Policy {
    origins: Vec<Url>,
    lan: bool,
}
impl Policy {
    pub fn new(origins: &str, lan: bool) -> Result<Self> {
        let mut configured = vec![];
        for origin in origins.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let url = Url::parse(origin)?;
            ensure!(
                ["http", "https"].contains(&url.scheme())
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "ADB_STUDIO_ORIGINS 需填写完整的 HTTP／HTTPS 来源网址，不能带路径或账号"
            );
            configured.push(url);
        }
        Ok(Self {
            origins: configured,
            lan,
        })
    }
    fn matches_host(url: &Url, host: &str) -> bool {
        Url::parse(&format!("{}://{host}", url.scheme())).is_ok_and(|candidate| {
            candidate.host_str() == url.host_str()
                && candidate.port_or_known_default() == url.port_or_known_default()
                && candidate.username().is_empty()
                && candidate.password().is_none()
                && candidate.path() == "/"
                && candidate.query().is_none()
                && candidate.fragment().is_none()
        })
    }
    pub fn secure(&self, headers: &HeaderMap) -> bool {
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        self.origins
            .iter()
            .any(|o| o.scheme() == "https" && Self::matches_host(o, host))
    }
    pub fn allows(&self, headers: &HeaderMap) -> bool {
        if headers
            .get("sec-fetch-site")
            .is_some_and(|s| s == "cross-site")
        {
            return false;
        }
        let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
            return false;
        };
        if let Some(expected) = self.origins.iter().find(|o| Self::matches_host(o, host)) {
            return headers
                .get(header::ORIGIN)
                .is_none_or(|s| s == expected.origin().ascii_serialization().as_str());
        }
        let Ok(url) = Url::parse(&format!("http://{host}")) else {
            return false;
        };
        let Some(name) = url.host_str() else {
            return false;
        };
        let local = ["localhost", "127.0.0.1"].contains(&name);
        let lan = self.origins.is_empty() && self.lan && name.parse::<std::net::Ipv4Addr>().is_ok();
        if !(local || lan) || !host.contains(':') || !Self::matches_host(&url, host) {
            return false;
        }
        headers
            .get(header::ORIGIN)
            .is_none_or(|s| s == url.origin().ascii_serialization().as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(host: &str, origin: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, host.parse().unwrap());
        if let Some(origin) = origin {
            h.insert(header::ORIGIN, origin.parse().unwrap());
        }
        h
    }
    #[test]
    fn https_and_proxy_hosts_are_explicit() {
        let p = Policy::new("https://studio.home:8443", true).unwrap();
        assert!(p.allows(&headers("studio.home:8443", None)));
        assert!(p.allows(&headers(
            "studio.home:8443",
            Some("https://studio.home:8443")
        )));
        assert!(p.secure(&headers("studio.home:8443", None)));
        for (host, origin) in [
            ("studio.home:8443", Some("http://studio.home:8443")),
            ("evil.home:8443", None),
            ("192.168.2.25:17841", None),
            ("studio.home:8443/path", None),
        ] {
            assert!(!p.allows(&headers(host, origin)), "{host:?} {origin:?}");
        }
        assert!(p.allows(&headers("127.0.0.1:17841", None)));
    }
    #[test]
    fn legacy_lan_and_bad_configuration() {
        assert!(Policy::new("", true).unwrap().allows(&headers(
            "192.168.2.25:17841",
            Some("http://192.168.2.25:17841")
        )));
        assert!(
            !Policy::new("", false)
                .unwrap()
                .allows(&headers("192.168.2.25:17841", None))
        );
        for o in [
            "https://a/path",
            "https://u:p@a",
            "file:///tmp/test",
            "https://a?x=1",
        ] {
            assert!(Policy::new(o, true).is_err());
        }
    }
}
