use std::{
    net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs as _},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

use crate::endpoint::TailnetEndpoint;

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const LINK_PREFIX: &str = "zz://host/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub name: String,
    pub peer: String,
    pub address: Option<IpAddr>,
    pub port: u16,
    pub os: String,
}

impl Offer {
    #[must_use]
    pub fn endpoint(&self) -> TailnetEndpoint {
        TailnetEndpoint {
            peer: self.peer.clone(),
            port: (self.port != TailnetEndpoint::DEFAULT_PORT).then_some(self.port),
        }
    }
}

#[must_use]
pub fn offers_json(offers: &[Offer]) -> String {
    Value::Array(
        offers
            .iter()
            .map(|offer| {
                json!({
                    "name": offer.name,
                    "peer": offer.peer,
                    "address": offer.address.map(|address| address.to_string()),
                    "port": offer.port,
                    "os": offer.os,
                })
            })
            .collect(),
    )
    .to_string()
}

#[must_use]
pub fn parse_offers(text: &str) -> Vec<Offer> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let peer = item["peer"].as_str()?.to_owned();
            let port = u16::try_from(item["port"].as_u64()?).ok()?;
            Some(Offer {
                name: item["name"].as_str().unwrap_or(&peer).to_owned(),
                address: item["address"].as_str().and_then(|text| text.parse().ok()),
                os: item["os"].as_str().unwrap_or_default().to_owned(),
                peer,
                port,
            })
        })
        .collect()
}

#[must_use]
pub fn reachable(offers: Vec<Offer>) -> Vec<Offer> {
    thread::scope(|scope| {
        let probes: Vec<_> = offers
            .into_iter()
            .map(|offer| {
                scope.spawn(move || {
                    let target = match offer.address {
                        Some(address) => SocketAddr::new(address, offer.port),
                        None => (offer.peer.as_str(), offer.port)
                            .to_socket_addrs()
                            .ok()?
                            .next()?,
                    };
                    TcpStream::connect_timeout(&target, PROBE_TIMEOUT).ok()?;
                    Some(offer)
                })
            })
            .collect();
        probes
            .into_iter()
            .filter_map(|probe| probe.join().ok().flatten())
            .collect()
    })
}

#[must_use]
pub fn host_link(endpoint: &TailnetEndpoint) -> String {
    let port = endpoint.port.unwrap_or(TailnetEndpoint::DEFAULT_PORT);
    format!("{LINK_PREFIX}{}:{port}", endpoint.peer)
}

#[must_use]
pub fn endpoint_from_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix(LINK_PREFIX)?.trim_end_matches('/');
    let endpoint = crate::Endpoint::parse(&format!("tailnet://{rest}")).ok()?;
    matches!(endpoint, crate::Endpoint::Tailnet(_)).then(|| endpoint.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_round_trip_and_links_carry_the_port() {
        let offers = vec![
            Offer {
                name: "alienware".to_owned(),
                peer: "alienware.example.ts.net".to_owned(),
                address: Some("100.99.198.46".parse().unwrap()),
                port: 9922,
                os: "linux".to_owned(),
            },
            Offer {
                name: "xps".to_owned(),
                peer: "xps.example.ts.net".to_owned(),
                address: None,
                port: 9923,
                os: String::new(),
            },
        ];
        assert_eq!(parse_offers(&offers_json(&offers)), offers);
        assert_eq!(parse_offers("not json"), Vec::new());
        let endpoint = TailnetEndpoint {
            peer: "xps.example.ts.net".to_owned(),
            port: Some(9930),
        };
        let link = host_link(&endpoint);
        assert_eq!(link, "zz://host/xps.example.ts.net:9930");
        assert_eq!(
            endpoint_from_link(&link).as_deref(),
            Some("tailnet://xps.example.ts.net:9930")
        );
        for url in [
            "zz://attach/main",
            "zz://host/",
            "zz://host/a b",
            "zz://host/box:0",
        ] {
            assert_eq!(endpoint_from_link(url), None, "{url}");
        }
    }
}
