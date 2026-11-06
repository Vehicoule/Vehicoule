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

#[cfg(any(feature = "good", feature = "undeclared-export"))]
impl exports::vehicoule::plugin::search::Guest for Fixture {
    fn search(
        query: String,
        _kinds: Vec<vehicoule::plugin::types::ItemKind>,
        _cursor: Option<String>,
    ) -> Result<exports::vehicoule::plugin::search::Page, vehicoule::plugin::types::Error> {
        Err(vehicoule::plugin::types::Error::InvalidInput(query))
    }

    fn suggest(_prefix: String) -> Result<Vec<String>, vehicoule::plugin::types::Error> {
        Ok(Vec::new())
    }
}

export!(Fixture);
