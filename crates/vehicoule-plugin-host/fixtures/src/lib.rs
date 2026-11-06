//! Test plugins for `vehicoule-plugin-host`. Exactly one feature is enabled per build.

#[cfg(not(any(
    feature = "extra-import",
    feature = "undeclared-export",
    feature = "good"
)))]
wit_bindgen::generate!({ path: "../../../wit", world: "vehicoule:plugin/base@0.1.0" });

#[cfg(any(feature = "good", feature = "undeclared-export"))]
wit_bindgen::generate!({ path: "../../../wit", world: "vehicoule:plugin/search-plugin@0.1.0" });

#[cfg(feature = "extra-import")]
wit_bindgen::generate!({
    path: "../../../wit",
    inline: "
        package fixture:plugin;

        interface outside {
            open: func(path: string) -> u32;
        }

        world extra-import {
            include vehicoule:plugin/base@0.1.0;
            import outside;
        }
    ",
    world: "fixture:plugin/extra-import",
    generate_all,
});

use exports::vehicoule::plugin::plugin::{Guest, Manifest};
use vehicoule::plugin::types::{Capability, Domain};

struct Fixture;

fn manifest(name: &str, capabilities: Vec<Capability>, hosts: Vec<String>) -> Manifest {
    Manifest {
        id: String::from("fixture"),
        name: String::from(name),
        version: String::from("0.1.0"),
        domains: vec![Domain::Music],
        capabilities,
        http_hosts: hosts,
    }
}

fn hosts() -> Vec<String> {
    vec![
        String::from("api.example.org"),
        String::from("*.cdn.example.org"),
    ]
}

impl Guest for Fixture {
    fn describe() -> Manifest {
        behavior()
    }
}

#[cfg(feature = "good")]
fn behavior() -> Manifest {
    manifest("good", vec![Capability::Search], hosts())
}

#[cfg(feature = "undeclared-export")]
fn behavior() -> Manifest {
    manifest("undeclared", Vec::new(), hosts())
}

#[cfg(feature = "declares-missing-export")]
fn behavior() -> Manifest {
    manifest("missing", vec![Capability::Resolve], hosts())
}

#[cfg(feature = "bad-host-pattern")]
fn behavior() -> Manifest {
    let mut patterns = hosts();
    patterns.push(String::from("*"));
    manifest("bad", Vec::new(), patterns)
}

#[cfg(feature = "extra-import")]
fn behavior() -> Manifest {
    let _ = fixture::plugin::outside::open("/etc/passwd");
    manifest("extra", Vec::new(), hosts())
}

#[cfg(feature = "endless-loop")]
fn behavior() -> Manifest {
    // Keeps the shared helpers used in every build; never taken.
    if std::hint::black_box(false) {
        return manifest("never", Vec::new(), hosts());
    }
    let mut n: u64 = 0;
    loop {
        n = std::hint::black_box(n.wrapping_add(1));
    }
}

#[cfg(feature = "memory-bomb")]
fn behavior() -> Manifest {
    // Keeps the shared helpers used in every build; never taken.
    if std::hint::black_box(false) {
        return manifest("never", Vec::new(), hosts());
    }
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    loop {
        chunks.push(std::hint::black_box(vec![1u8; 1 << 20]));
    }
}

#[cfg(feature = "trap")]
fn behavior() -> Manifest {
    // Keeps the shared helpers used in every build; never taken.
    if std::hint::black_box(false) {
        return manifest("never", Vec::new(), hosts());
    }
    panic!("trap on purpose")
}

#[cfg(feature = "fetch-in-describe")]
fn behavior() -> Manifest {
    use vehicoule::plugin::http::{HttpError, Method, Request, fetch};
    let answer = fetch(&Request {
        method: Method::Get,
        url: String::from("https://api.example.org/"),
        headers: Vec::new(),
        body: None,
    });
    let name = match answer {
        Err(HttpError::RefusedHost) => "refused-host",
        Err(_) => "other-error",
        Ok(_) => "reached",
    };
    manifest(name, Vec::new(), hosts())
}

/// `search` does what its query says, so one plugin covers every call the tests make: `trap`, `loop`, `grow`,
/// `count` (how many calls this instance has seen), `big:N` (a title of N bytes), `error:KIND`; any other query
/// returns one item of each requested kind, titled after the query.
#[cfg(any(feature = "good", feature = "undeclared-export"))]
impl exports::vehicoule::plugin::search::Guest for Fixture {
    fn search(
        query: String,
        kinds: Vec<vehicoule::plugin::types::ItemKind>,
        cursor: Option<String>,
    ) -> Result<exports::vehicoule::plugin::search::Page, vehicoule::plugin::types::Error> {
        use std::sync::atomic::{AtomicU32, Ordering};
        use vehicoule::plugin::types::{
            Album, AlbumRef, Artist, ArtistRef, Artwork, Error, Item, ItemKind, Playlist, Track,
        };

        static CALLS: AtomicU32 = AtomicU32::new(0);
        let calls = CALLS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);

        let track = |title: String| Track {
            id: String::from("track-1"),
            title,
            artists: vec![ArtistRef {
                id: String::from("artist-1"),
                name: String::from("Artist"),
            }],
            album: Some(AlbumRef {
                id: String::from("album-1"),
                title: String::from("Album"),
            }),
            duration_ms: Some(215_000),
            isrc: Some(String::from("FRZ039800212")),
            artwork: vec![Artwork {
                url: String::from("https://a.cdn.example.org/cover.jpg"),
                width: Some(544),
                height: None,
            }],
        };
        let page = |items: Vec<Item>| exports::vehicoule::plugin::search::Page {
            items,
            next: Some(cursor.clone().map_or(String::from("page-2"), |c| c + "+")),
        };

        match query.as_str() {
            "trap" => panic!("trap on purpose"),
            "loop" => {
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "grow" => {
                let mut chunks: Vec<Vec<u8>> = Vec::new();
                loop {
                    chunks.push(std::hint::black_box(vec![1u8; 1 << 20]));
                }
            }
            "count" => Ok(page(vec![Item::Track(track(calls.to_string()))])),
            "error:not-found" => Err(Error::NotFound),
            "error:unsupported" => Err(Error::Unsupported),
            "error:rate-limited" => Err(Error::RateLimited(Some(1_500))),
            "error:invalid-input" => Err(Error::InvalidInput(String::from("bad query"))),
            "error:upstream" => Err(Error::Upstream(String::from("service down"))),
            q if q.starts_with("big:") => {
                let n = q.trim_start_matches("big:").parse().unwrap_or(0);
                Ok(page(vec![Item::Track(track("a".repeat(n)))]))
            }
            _ => Ok(page(
                kinds
                    .iter()
                    .map(|kind| match kind {
                        ItemKind::Track => Item::Track(track(query.clone())),
                        ItemKind::Album => Item::Album(Album {
                            id: String::from("album-1"),
                            title: query.clone(),
                            artists: Vec::new(),
                            year: Some(1998),
                            artwork: Vec::new(),
                        }),
                        ItemKind::Artist => Item::Artist(Artist {
                            id: String::from("artist-1"),
                            name: query.clone(),
                            artwork: Vec::new(),
                        }),
                        ItemKind::Playlist => Item::Playlist(Playlist {
                            id: String::from("playlist-1"),
                            title: query.clone(),
                            owner: None,
                            artwork: Vec::new(),
                        }),
                    })
                    .collect(),
            )),
        }
    }

    fn suggest(prefix: String) -> Result<Vec<String>, vehicoule::plugin::types::Error> {
        Ok(vec![prefix.clone() + " live", prefix + " remix"])
    }
}

export!(Fixture);
