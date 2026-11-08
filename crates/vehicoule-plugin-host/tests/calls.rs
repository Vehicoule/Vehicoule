//! Capability calls against the test plugins of `fixtures/` (docs/plugin-host-spec.md, sections 3, 6, 7 and 9).
//!
//! The `good` plugin's `search` does what its query says (see `fixtures/src/lib.rs`), so one plugin covers every
//! behavior a call can have.

mod common;

use std::time::Duration;

use common::{
    FUEL, FakeNetwork, MAX_LOG_MESSAGE, MAX_LOG_MESSAGES, MAX_MEMORY, MAX_OUTPUT, MAX_REQUESTS,
    MAX_RESPONSE_BODY, REQUEST_DEADLINE, fixture, host, host_on, host_ticking,
};
use vehicoule_plugin_host::{
    AlbumRef, ArtistRef, Artwork, ByteSize, CallReport, Capability, Fuel, HttpResponse,
    InstalledPlugin, Item, ItemKind, LogLevel, Method, NetworkError, PluginError, PluginFailure,
    PluginHost, ResultFault, Track,
};

/// A host with the `good` plugin installed; an error says which step failed, and the calling test unwraps it.
fn good() -> Result<(PluginHost, InstalledPlugin), String> {
    let host = host().map_err(|e| e.to_string())?;
    let plugin = host.install(&fixture("good")?).map_err(|e| e.to_string())?;
    Ok((host, plugin))
}

#[test]
fn a_search_returns_one_item_per_requested_kind_with_every_field() {
    let (host, plugin) = good().unwrap();
    let page = host
        .search(
            &plugin,
            "daft",
            &[ItemKind::Track, ItemKind::Album, ItemKind::Artist],
            None,
        )
        .outcome
        .unwrap();
    assert_eq!(page.next.as_deref(), Some("page-2"));
    assert_eq!(page.items.len(), 3);
    let Item::Track(track) = &page.items[0] else {
        panic!("the first item must be a track: {:?}", page.items[0]);
    };
    assert_eq!(track.id, "track-1");
    assert_eq!(track.title, "daft");
    assert_eq!(track.artists.len(), 1);
    assert_eq!(
        (track.artists[0].id.as_str(), track.artists[0].name.as_str()),
        ("artist-1", "Artist")
    );
    assert_eq!(
        track
            .album
            .as_ref()
            .map(|a| (a.id.as_str(), a.title.as_str())),
        Some(("album-1", "Album"))
    );
    assert_eq!(track.duration, Some(Duration::from_secs(215)));
    assert_eq!(track.isrc.as_deref(), Some("FRZ039800212"));
    assert_eq!(track.artwork.len(), 1);
    assert_eq!(track.artwork[0].url, "https://a.cdn.example.org/cover.jpg");
    assert_eq!(
        (track.artwork[0].width, track.artwork[0].height),
        (Some(544), None)
    );
    let Item::Album(album) = &page.items[1] else {
        panic!("the second item must be an album: {:?}", page.items[1]);
    };
    assert_eq!(
        (album.id.as_str(), album.title.as_str(), album.year),
        ("album-1", "daft", Some(1998))
    );
    let Item::Artist(artist) = &page.items[2] else {
        panic!("the third item must be an artist: {:?}", page.items[2]);
    };
    assert_eq!(
        (artist.id.as_str(), artist.name.as_str()),
        ("artist-1", "daft")
    );
}

#[test]
fn the_cursor_and_the_kinds_reach_the_plugin() {
    let (host, plugin) = good().unwrap();
    let page = host
        .search(&plugin, "daft", &[ItemKind::Playlist], Some("page-2"))
        .outcome
        .unwrap();
    // The plugin answers `next` = cursor + "+": it saw the cursor.
    assert_eq!(page.next.as_deref(), Some("page-2+"));
    assert_eq!(page.items.len(), 1);
    assert!(
        matches!(&page.items[0], Item::Playlist(p) if p.title == "daft" && p.owner.is_none()),
        "{:?}",
        page.items
    );
    let empty = host.search(&plugin, "daft", &[], None).outcome.unwrap();
    assert!(empty.items.is_empty(), "{:?}", empty.items);
}

#[test]
fn suggestions_come_back_in_the_plugin_order() {
    let (host, plugin) = good().unwrap();
    assert_eq!(
        host.suggest(&plugin, "daft").outcome.unwrap(),
        ["daft live", "daft remix"]
    );
}

#[test]
fn each_error_of_the_plugin_comes_back_as_itself() {
    let (host, plugin) = good().unwrap();
    let cases = [
        ("error:not-found", PluginError::NotFound),
        ("error:unsupported", PluginError::Unsupported),
        (
            "error:rate-limited",
            PluginError::RateLimited {
                retry_after: Some(Duration::from_millis(1_500)),
            },
        ),
        (
            "error:invalid-input",
            PluginError::InvalidInput("bad query".to_owned()),
        ),
        (
            "error:upstream",
            PluginError::Upstream("service down".to_owned()),
        ),
    ];
    for (query, expected) in cases {
        let failure = host
            .search(&plugin, query, &[ItemKind::Track], None)
            .outcome
            .unwrap_err();
        assert_eq!(failure, PluginFailure::Plugin(expected), "{query}");
    }
}

#[test]
fn an_endless_call_is_stopped_by_fuel() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "loop", &[], None).outcome.unwrap_err();
    assert_eq!(failure, PluginFailure::FuelExhausted);
}

#[test]
fn a_call_that_grows_memory_without_bound_is_stopped_at_the_ceiling() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "grow", &[], None).outcome.unwrap_err();
    assert_eq!(failure, PluginFailure::MemoryLimit);
}

#[test]
fn a_trap_fails_the_call_and_the_next_call_works() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "trap", &[], None).outcome.unwrap_err();
    assert!(matches!(failure, PluginFailure::Trap(_)), "{failure:?}");
    let page = host
        .search(&plugin, "after", &[ItemKind::Track], None)
        .outcome
        .unwrap();
    assert!(
        matches!(&page.items[..], [Item::Track(t)] if t.title == "after"),
        "{:?}",
        page.items
    );
}

#[test]
fn every_budget_is_whole_again_on_the_next_call() {
    // A call that spent all its fuel or memory leaves the next call its full budget.
    let (host, plugin) = good().unwrap();
    assert!(host.search(&plugin, "loop", &[], None).outcome.is_err());
    assert!(host.search(&plugin, "grow", &[], None).outcome.is_err());
    assert!(
        host.search(&plugin, "daft", &[ItemKind::Track], None)
            .outcome
            .is_ok()
    );
}

#[test]
fn nothing_a_plugin_keeps_in_memory_survives_to_the_next_call() {
    // The plugin counts its calls in a static: in a fresh instance per call, it always counts one.
    let (host, plugin) = good().unwrap();
    for _ in 0..3 {
        let page = host.search(&plugin, "count", &[], None).outcome.unwrap();
        assert!(
            matches!(&page.items[..], [Item::Track(t)] if t.title == "1"),
            "{:?}",
            page.items
        );
    }
}

/// The size of a `big:N` answer besides its N-byte title: one item (1), `track-1` (7), one artist (1) `artist-1` (8)
/// `Artist` (6), `album-1` (7), `Album` (5), the ISRC (12), one artwork (1) and its URL (35), and `next` = `page-2`
/// (6).
const BIG_OVERHEAD: u64 = 89;

#[test]
fn an_answer_exactly_at_the_output_ceiling_is_returned() {
    let (host, plugin) = good().unwrap();
    let title = MAX_OUTPUT - BIG_OVERHEAD;
    let page = host
        .search(&plugin, &format!("big:{title}"), &[], None)
        .outcome
        .unwrap();
    assert!(
        matches!(&page.items[..], [Item::Track(t)] if t.title.len() == usize::try_from(title).unwrap()),
        "one track with a title of {title} bytes"
    );
}

#[test]
fn an_answer_over_the_output_ceiling_is_refused() {
    let (host, plugin) = good().unwrap();
    let title = MAX_OUTPUT - BIG_OVERHEAD + 1;
    let failure = host
        .search(&plugin, &format!("big:{title}"), &[], None)
        .outcome
        .unwrap_err();
    assert_eq!(
        failure,
        PluginFailure::OutputTooLarge {
            size: ByteSize::new(MAX_OUTPUT + 1),
            limit: ByteSize::new(MAX_OUTPUT),
        }
    );
}

#[test]
fn a_plugin_that_does_not_provide_search_is_not_called_for_it() {
    let host = host().unwrap();
    // This plugin declares and exports no capability.
    let plugin = host
        .install(&fixture("fetch-in-describe").unwrap())
        .unwrap();
    assert_eq!(
        host.search(&plugin, "daft", &[ItemKind::Track], None)
            .outcome
            .unwrap_err(),
        PluginFailure::NotProvided(Capability::Search)
    );
    assert_eq!(
        host.suggest(&plugin, "daft").outcome.unwrap_err(),
        PluginFailure::NotProvided(Capability::Search)
    );
}

#[test]
fn a_long_list_of_empty_items_counts_toward_the_output_ceiling() {
    // `empty:N` returns N artists with empty strings and no artwork: each counts one, plus `next` = `page-2` (6).
    let (host, plugin) = good().unwrap();
    let fits = MAX_OUTPUT - 6;
    let page = host
        .search(&plugin, &format!("empty:{fits}"), &[], None)
        .outcome
        .unwrap();
    assert_eq!(page.items.len(), usize::try_from(fits).unwrap());
    let failure = host
        .search(&plugin, &format!("empty:{}", fits + 1), &[], None)
        .outcome
        .unwrap_err();
    assert_eq!(
        failure,
        PluginFailure::OutputTooLarge {
            size: ByteSize::new(MAX_OUTPUT + 1),
            limit: ByteSize::new(MAX_OUTPUT),
        }
    );
}

// The network (docs/plugin-host-spec.md, sections 5, 6 and 9). The `good` plugin declares `api.example.org` and
// `*.cdn.example.org`, and titles its answer after what each of its requests returned.

/// The `good` plugin on a host whose network is `network`.
fn good_on(network: &std::sync::Arc<FakeNetwork>) -> Result<(PluginHost, InstalledPlugin), String> {
    let host = host_on(network.clone()).map_err(|e| e.to_string())?;
    let plugin = host.install(&fixture("good")?).map_err(|e| e.to_string())?;
    Ok((host, plugin))
}

/// The title of the single track the plugin answered `query` with.
fn answer(
    host: &PluginHost,
    plugin: &InstalledPlugin,
    query: &str,
) -> Result<String, PluginFailure> {
    let page = host.search(plugin, query, &[], None).outcome?;
    match page.items.as_slice() {
        [Item::Track(track)] => Ok(track.title.clone()),
        other => Err(PluginFailure::Runtime(format!(
            "unexpected items: {other:?}"
        ))),
    }
}

#[test]
fn a_request_to_a_declared_host_reaches_the_network_as_sent_and_its_response_comes_back() {
    let network = FakeNetwork::new(|_| {
        Ok(HttpResponse::new(
            200,
            vec![("content-type".to_owned(), "text/plain".to_owned())],
            b"hello".to_vec(),
        ))
    });
    let (host, plugin) = good_on(&network).unwrap();
    let title = answer(
        &host,
        &plugin,
        "fetch:1:https://API.example.org/v1?q=a#frag",
    )
    .unwrap();
    assert_eq!(title, "200:hello");
    let requests = network.requests();
    assert_eq!(requests.len(), 1);
    let (request, limits) = &requests[0];
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.url.as_str(), "https://api.example.org/v1?q=a");
    assert_eq!(request.url.host(), "api.example.org");
    assert_eq!(request.headers, [("x-test".to_owned(), "1".to_owned())]);
    assert_eq!(request.body, None);
    assert_eq!(limits.max_body, ByteSize::new(MAX_RESPONSE_BODY));
    assert_eq!(limits.deadline, REQUEST_DEADLINE);
}

#[test]
fn a_post_carries_its_body_and_headers() {
    let network = FakeNetwork::new(|request| {
        Ok(HttpResponse::new(
            201,
            Vec::new(),
            request.body.clone().unwrap_or_default(),
        ))
    });
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "post:https://api.example.org/items").unwrap(),
        "201:payload"
    );
    let requests = network.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0.method, Method::Post);
    assert_eq!(requests[0].0.body.as_deref(), Some(&b"payload"[..]));
    assert_eq!(
        requests[0].0.headers,
        [("content-type".to_owned(), "text/plain".to_owned())]
    );
}

#[test]
fn a_subdomain_pattern_lets_subdomains_through_and_not_its_own_name() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://a.cdn.example.org/x").unwrap(),
        "200:ok"
    );
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://cdn.example.org/x").unwrap(),
        "refused-host"
    );
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn no_refused_request_reaches_the_network() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    let cases = [
        ("https://evil.org/", "refused-host"),
        ("https://api.example.org.evil.org/", "refused-host"),
        ("https://example.org/", "refused-host"),
        ("http://api.example.org/", "refused-scheme"),
        ("HTTPS://api.example.org/", "refused-scheme"),
        ("ftp://api.example.org/", "refused-scheme"),
        ("https://127.0.0.1/", "refused-host"),
        ("https://[::1]/", "refused-host"),
        ("https://api.example.org:443/", "refused-host"),
        ("https://api.example.org:8443/", "refused-host"),
        ("https://user@api.example.org/", "refused-host"),
        ("https://evil.org\\@api.example.org/", "refused-host"),
        ("https://api.example.org\\.evil.org/", "refused-host"),
    ];
    for (url, expected) in cases {
        assert_eq!(
            answer(&host, &plugin, &format!("fetch:1:{url}")).unwrap(),
            expected,
            "{url}"
        );
    }
    assert!(network.requests().is_empty(), "{:?}", network.requests());
}

#[test]
fn a_host_header_or_a_malformed_header_never_reaches_the_network() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "header:Host:https://api.example.org/").unwrap(),
        "refused-host"
    );
    assert_eq!(
        answer(&host, &plugin, "header:x bad:https://api.example.org/").unwrap(),
        "transport:header `x bad` cannot be sent"
    );
    assert!(network.requests().is_empty(), "{:?}", network.requests());
    assert_eq!(
        answer(&host, &plugin, "header:x-good:https://api.example.org/").unwrap(),
        "200:ok"
    );
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn a_redirect_goes_back_to_the_plugin_and_is_not_followed() {
    let network = FakeNetwork::new(|_| {
        Ok(HttpResponse::new(
            302,
            vec![("location".to_owned(), "https://evil.org/".to_owned())],
            Vec::new(),
        ))
    });
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap(),
        "302:"
    );
    let requests = network.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0.url.as_str(), "https://api.example.org/");
}

#[test]
fn the_request_budget_is_exact_and_the_extra_request_never_leaves() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    let all = answer(
        &host,
        &plugin,
        &format!("fetch:{MAX_REQUESTS}:https://api.example.org/"),
    )
    .unwrap();
    assert_eq!(all, ["200:ok"; 3].join(";"));
    let over = answer(
        &host,
        &plugin,
        &format!("fetch:{}:https://api.example.org/", MAX_REQUESTS + 1),
    )
    .unwrap_err();
    assert_eq!(over, PluginFailure::TooManyRequests);
    // 3 from the first call, 3 from the second: the fourth never reached the network.
    assert_eq!(
        network.requests().len(),
        usize::try_from(2 * MAX_REQUESTS).unwrap()
    );
}

#[test]
fn refused_requests_do_not_count_toward_the_budget() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    let title = answer(&host, &plugin, "fetch:5:https://evil.org/").unwrap();
    assert_eq!(title, ["refused-host"; 5].join(";"));
    assert!(network.requests().is_empty(), "{:?}", network.requests());
}

#[test]
fn a_body_exactly_at_its_limit_comes_back() {
    let size = usize::try_from(MAX_RESPONSE_BODY).unwrap();
    let network =
        FakeNetwork::new(move |_| Ok(HttpResponse::new(200, Vec::new(), vec![b'b'; size])));
    let (host, plugin) = good_on(&network).unwrap();
    let title = answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap();
    assert_eq!(title, format!("200:{}", "b".repeat(size)));
}

#[test]
fn a_body_over_its_limit_ends_the_call() {
    // A network that ignores the limit is caught by the host's own check.
    let size = usize::try_from(MAX_RESPONSE_BODY + 1).unwrap();
    let network =
        FakeNetwork::new(move |_| Ok(HttpResponse::new(200, Vec::new(), vec![b'b'; size])));
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap_err(),
        PluginFailure::ResponseTooLarge
    );
    // A network that stops reading reports it, with the same failure.
    let network = FakeNetwork::new(|_| Err(NetworkError::BodyTooLarge));
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap_err(),
        PluginFailure::ResponseTooLarge
    );
}

#[test]
fn a_timeout_ends_the_call_and_is_not_retried() {
    let network = FakeNetwork::new(|_| Err(NetworkError::Timeout));
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:2:https://api.example.org/").unwrap_err(),
        PluginFailure::RequestTimeout
    );
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn a_transport_failure_goes_back_to_the_plugin() {
    let network = FakeNetwork::new(|_| Err(NetworkError::Transport("no route".to_owned())));
    let (host, plugin) = good_on(&network).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:2:https://api.example.org/").unwrap(),
        "transport:no route;transport:no route"
    );
    assert_eq!(network.requests().len(), 2);
}

/// A host whose calls last `wall_time` ticks, with the `good` plugin installed, on a network that answers `200 ok`
/// and lets `ticks_per_request` ticks pass during each request.
fn good_ticking(
    wall_time: u64,
    ticks_per_request: u64,
) -> Result<(PluginHost, InstalledPlugin, std::sync::Arc<FakeNetwork>), String> {
    let network = common::ok_network();
    let host = host_ticking(&network, wall_time, ticks_per_request).map_err(|e| e.to_string())?;
    let plugin = host.install(&fixture("good")?).map_err(|e| e.to_string())?;
    Ok((host, plugin, network))
}

#[test]
fn a_call_that_reaches_its_wall_time_is_stopped() {
    let (host, plugin, network) = good_ticking(3, 3).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:2:https://api.example.org/").unwrap_err(),
        PluginFailure::WallTime
    );
    // Stopped when the plugin got control back after the first request, before it could send the second.
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn a_call_one_tick_short_of_its_wall_time_returns() {
    let (host, plugin, network) = good_ticking(3, 2).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap(),
        "200:ok"
    );
    // Each call counts its own ticks: the two ticks of the first call are not charged to the second.
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap(),
        "200:ok"
    );
    assert_eq!(network.requests().len(), 2);
}

#[test]
fn a_call_after_a_wall_time_stop_gets_its_whole_wall_time() {
    let (host, plugin, _network) = good_ticking(3, 3).unwrap();
    assert_eq!(
        answer(&host, &plugin, "fetch:1:https://api.example.org/").unwrap_err(),
        PluginFailure::WallTime
    );
    let page = host
        .search(&plugin, "after", &[ItemKind::Track], None)
        .outcome
        .unwrap();
    assert!(
        matches!(&page.items[..], [Item::Track(t)] if t.title == "after"),
        "{:?}",
        page.items
    );
}

// The call's report (docs/plugin-host-spec.md, sections 7 and 9): what each call used, whether it returned or failed.

#[test]
fn a_call_reports_the_requests_it_sent_and_the_ones_the_sandbox_refused() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    let sent = host.search(&plugin, "fetch:2:https://api.example.org/", &[], None);
    assert!(sent.outcome.is_ok(), "{:?}", sent.outcome);
    assert_eq!(sent.report.requests_sent, 2);
    assert_eq!(sent.report.requests_refused, 0);
    let refused = host.search(&plugin, "fetch:3:https://evil.org/", &[], None);
    assert!(refused.outcome.is_ok(), "{:?}", refused.outcome);
    assert_eq!(refused.report.requests_sent, 0);
    assert_eq!(refused.report.requests_refused, 3);
    let header = host.search(&plugin, "header:Host:https://api.example.org/", &[], None);
    assert_eq!(header.report.requests_refused, 1);
    assert_eq!(network.requests().len(), 2);
}

#[test]
fn a_call_ended_by_its_request_budget_reports_what_it_sent_and_no_output() {
    let network = common::ok_network();
    let (host, plugin) = good_on(&network).unwrap();
    let over = host.search(
        &plugin,
        &format!("fetch:{}:https://api.example.org/", MAX_REQUESTS + 1),
        &[],
        None,
    );
    assert_eq!(over.outcome, Err(PluginFailure::TooManyRequests));
    // The request over the budget never left, so it is neither sent nor refused: the failure names it.
    assert_eq!(over.report.requests_sent, MAX_REQUESTS);
    assert_eq!(over.report.requests_refused, 0);
    assert_eq!(over.report.output, None);
}

#[test]
fn the_fuel_a_call_used_is_reported_the_same_on_every_run_and_grows_with_the_work() {
    let (host, plugin) = good().unwrap();
    let small = host.search(&plugin, "empty:10", &[], None).report.fuel_used;
    assert_eq!(
        host.search(&plugin, "empty:10", &[], None).report.fuel_used,
        small
    );
    let large = host
        .search(&plugin, "empty:1000", &[], None)
        .report
        .fuel_used;
    assert!(
        Fuel::new(0) < small && small < large && large < Fuel::new(FUEL),
        "{small:?} {large:?}"
    );
    let endless = host.search(&plugin, "loop", &[], None);
    assert_eq!(endless.outcome, Err(PluginFailure::FuelExhausted));
    assert_eq!(endless.report.fuel_used, Fuel::new(FUEL));
}

#[test]
fn the_memory_peak_is_reported_and_never_over_the_ceiling() {
    let (host, plugin) = good().unwrap();
    let small = host
        .search(&plugin, "daft", &[ItemKind::Track], None)
        .report
        .memory_peak;
    let grow = host.search(&plugin, "grow", &[], None);
    assert_eq!(grow.outcome, Err(PluginFailure::MemoryLimit));
    let peak = grow.report.memory_peak;
    assert!(
        ByteSize::new(0) < small && small < peak && peak <= ByteSize::new(MAX_MEMORY),
        "{small:?} {peak:?}"
    );
}

#[test]
fn the_output_size_is_reported_for_a_value_an_error_or_an_answer_over_the_ceiling() {
    let (host, plugin) = good().unwrap();
    let at = host.search(
        &plugin,
        &format!("big:{}", MAX_OUTPUT - BIG_OVERHEAD),
        &[],
        None,
    );
    assert!(at.outcome.is_ok());
    assert_eq!(at.report.output, Some(ByteSize::new(MAX_OUTPUT)));
    let over = host.search(
        &plugin,
        &format!("big:{}", MAX_OUTPUT - BIG_OVERHEAD + 1),
        &[],
        None,
    );
    assert!(matches!(
        over.outcome,
        Err(PluginFailure::OutputTooLarge { .. })
    ));
    assert_eq!(over.report.output, Some(ByteSize::new(MAX_OUTPUT + 1)));
    // `service down` is 12 bytes.
    let error = host.search(&plugin, "error:upstream", &[], None);
    assert_eq!(error.report.output, Some(ByteSize::new(12)));
    let trap = host.search(&plugin, "trap", &[], None);
    assert!(matches!(trap.outcome, Err(PluginFailure::Trap(_))));
    assert_eq!(trap.report.output, None);
}

#[test]
fn a_call_to_a_capability_the_plugin_does_not_provide_reports_nothing_used() {
    let host = host().unwrap();
    let plugin = host
        .install(&fixture("fetch-in-describe").unwrap())
        .unwrap();
    let call = host.suggest(&plugin, "daft");
    assert_eq!(
        call.outcome,
        Err(PluginFailure::NotProvided(Capability::Search))
    );
    assert_eq!(call.report, CallReport::default());
}

// Logs (docs/plugin-host-spec.md, section 5): bounded per call, tagged with the plugin's id, in the call's report.

#[test]
fn what_a_plugin_logs_comes_back_in_order_tagged_with_its_id_and_level() {
    let (host, plugin) = good().unwrap();
    let call = host.search(&plugin, "log:warn:2:slow", &[], None);
    assert!(call.outcome.is_ok(), "{:?}", call.outcome);
    let logs = &call.report.logs;
    assert_eq!(logs.len(), 2);
    for entry in logs {
        assert_eq!(entry.plugin.as_str(), "fixture");
        assert_eq!(entry.level, LogLevel::Warn);
        assert_eq!(entry.message, "slow");
        assert!(!entry.truncated);
    }
    assert_eq!(call.report.logs_dropped, 0);
}

#[test]
fn messages_over_the_count_are_dropped_and_counted() {
    let (host, plugin) = good().unwrap();
    let call = host.search(
        &plugin,
        &format!("log:info:{}:x", MAX_LOG_MESSAGES + 2),
        &[],
        None,
    );
    assert_eq!(
        call.report.logs.len(),
        usize::try_from(MAX_LOG_MESSAGES).unwrap()
    );
    assert_eq!(call.report.logs_dropped, 2);
    // The next call has its whole budget again.
    let next = host.search(&plugin, "log:info:1:x", &[], None);
    assert_eq!(next.report.logs.len(), 1);
    assert_eq!(next.report.logs_dropped, 0);
}

#[test]
fn a_message_at_its_limit_is_kept_whole_and_a_longer_one_is_cut() {
    let (host, plugin) = good().unwrap();
    let limit = usize::try_from(MAX_LOG_MESSAGE).unwrap();
    let at = host.search(
        &plugin,
        &format!("log:info:1:{}", "a".repeat(limit)),
        &[],
        None,
    );
    assert_eq!(at.report.logs[0].message, "a".repeat(limit));
    assert!(!at.report.logs[0].truncated);
    let over = host.search(
        &plugin,
        &format!("log:info:1:{}", "a".repeat(limit + 1)),
        &[],
        None,
    );
    assert_eq!(over.report.logs[0].message, "a".repeat(limit));
    assert!(over.report.logs[0].truncated);
}

#[test]
fn a_message_cannot_break_the_line_it_is_written_on() {
    let (host, plugin) = good().unwrap();
    let forged = host.search(&plugin, "log:error:1:a\nERROR forged", &[], None);
    assert_eq!(forged.report.logs[0].message, "a\u{FFFD}ERROR forged");
}

#[test]
fn what_a_plugin_logged_before_it_trapped_is_kept() {
    let (host, plugin) = good().unwrap();
    let call = host.search(&plugin, "log-trap", &[], None);
    assert!(
        matches!(call.outcome, Err(PluginFailure::Trap(_))),
        "{:?}",
        call.outcome
    );
    assert_eq!(call.report.logs.len(), 1);
    assert_eq!(call.report.logs[0].level, LogLevel::Error);
    assert_eq!(call.report.logs[0].message, "about to trap");
}

// Radio (docs/plugin-host-spec.md, sections 3, 6, 7 and 8): the `radio` plugin's `continuation` does what its seed's id
// says (see `fixtures/src/lib.rs`).

/// A host with the `radio` plugin installed.
fn radio_plugin() -> Result<(PluginHost, InstalledPlugin), String> {
    let host = host().map_err(|e| e.to_string())?;
    let plugin = host
        .install(&fixture("radio")?)
        .map_err(|e| e.to_string())?;
    Ok((host, plugin))
}

fn seed(id: &str) -> Track {
    let mut seed = Track::default();
    id.clone_into(&mut seed.id);
    "Seed".clone_into(&mut seed.title);
    seed
}

#[test]
fn the_radio_plugin_installs_with_the_radio_capability_only() {
    let (_, plugin) = radio_plugin().unwrap();
    assert_eq!(
        plugin
            .manifest()
            .capabilities()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [Capability::Radio]
    );
}

#[test]
fn every_field_of_the_seed_the_excluded_ids_and_the_count_reach_the_plugin() {
    let (host, plugin) = radio_plugin().unwrap();
    let mut artist = ArtistRef::default();
    artist.id = "ar1".to_owned();
    artist.name = "Artist One".to_owned();
    let mut second = ArtistRef::default();
    second.id = "ar2".to_owned();
    second.name = "Artist Two".to_owned();
    let mut album = AlbumRef::default();
    album.id = "al1".to_owned();
    album.title = "Album One".to_owned();
    let mut cover = Artwork::default();
    cover.url = "https://a.cdn.example.org/cover.jpg".to_owned();
    cover.width = Some(544);
    let mut thumbnail = Artwork::default();
    thumbnail.url = "https://a.cdn.example.org/thumb.jpg".to_owned();
    thumbnail.height = Some(60);
    let mut seed = seed("echo");
    seed.artists = vec![artist, second];
    seed.album = Some(album);
    seed.duration = Some(Duration::from_millis(215_999));
    seed.isrc = Some("FRZ039800212".to_owned());
    seed.artwork = vec![cover, thumbnail];

    let tracks = host
        .radio(&plugin, &seed, &["x1".to_owned(), "x2".to_owned()], 7)
        .outcome
        .unwrap();

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, "echoed");
    // The duration is in milliseconds, rounded down; an absent field arrives absent.
    assert_eq!(
        tracks[0].title,
        "title=Seed;artists=ar1=Artist One,ar2=Artist Two;album=Some(\"al1=Album One\");\
         duration=Some(215999);isrc=Some(\"FRZ039800212\");\
         artwork=https://a.cdn.example.org/cover.jpg@Some(544)xNone,\
         https://a.cdn.example.org/thumb.jpg@NonexSome(60);exclude=x1,x2;count=7"
    );
}

#[test]
fn absent_optional_fields_of_the_seed_arrive_absent() {
    let (host, plugin) = radio_plugin().unwrap();
    let tracks = host.radio(&plugin, &seed("echo"), &[], 0).outcome.unwrap();
    assert_eq!(
        tracks[0].title,
        "title=Seed;artists=;album=None;duration=None;isrc=None;artwork=;exclude=;count=0"
    );
}

#[test]
fn a_duration_past_the_contracts_milliseconds_saturates_instead_of_wrapping() {
    let (host, plugin) = radio_plugin().unwrap();
    let mut long = seed("echo");
    long.duration = Some(Duration::MAX);
    let tracks = host.radio(&plugin, &long, &[], 1).outcome.unwrap();
    assert!(
        tracks[0]
            .title
            .contains(&format!("duration=Some({})", u64::MAX)),
        "{}",
        tracks[0].title
    );
}

#[test]
fn the_tracks_come_back_in_the_plugins_order_with_every_field_read_back() {
    let (host, plugin) = radio_plugin().unwrap();
    let tracks = host.radio(&plugin, &seed("count"), &[], 3).outcome.unwrap();
    assert_eq!(
        tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["radio-1", "radio-2", "radio-3"]
    );
    assert!(tracks.iter().all(|t| t.title == "Seed"));
    assert_eq!(
        host.radio(&plugin, &seed("count"), &[], 0).outcome.unwrap(),
        Vec::<Track>::new()
    );
}

#[test]
fn each_error_of_the_radio_plugin_comes_back_as_itself() {
    let (host, plugin) = radio_plugin().unwrap();
    let cases = [
        ("error:not-found", PluginError::NotFound),
        ("error:unsupported", PluginError::Unsupported),
        (
            "error:rate-limited",
            PluginError::RateLimited {
                retry_after: Some(Duration::from_millis(1_500)),
            },
        ),
        (
            "error:invalid-input",
            PluginError::InvalidInput("bad seed".to_owned()),
        ),
        (
            "error:upstream",
            PluginError::Upstream("service down".to_owned()),
        ),
    ];
    for (id, expected) in cases {
        let failure = host.radio(&plugin, &seed(id), &[], 1).outcome.unwrap_err();
        assert_eq!(failure, PluginFailure::Plugin(expected), "{id}");
    }
}

#[test]
fn a_plugin_that_does_not_provide_radio_is_not_called_for_it_and_the_other_way_round() {
    let host = host().unwrap();
    let search_only = host.install(&fixture("good").unwrap()).unwrap();
    let call = host.radio(&search_only, &seed("count"), &[], 3);
    assert_eq!(
        call.outcome,
        Err(PluginFailure::NotProvided(Capability::Radio))
    );
    assert_eq!(call.report, CallReport::default());

    let radio_only = host.install(&fixture("radio").unwrap()).unwrap();
    let call = host.search(&radio_only, "daft", &[ItemKind::Track], None);
    assert_eq!(
        call.outcome.unwrap_err(),
        PluginFailure::NotProvided(Capability::Search)
    );
    assert_eq!(call.report, CallReport::default());
}

#[test]
fn a_radio_call_reaches_the_network_only_through_the_sandbox() {
    let network = common::ok_network();
    let host = host_on(network.clone()).unwrap();
    let plugin = host.install(&fixture("radio").unwrap()).unwrap();

    let call = host.radio(
        &plugin,
        &seed("fetch:https://api.example.org/radio"),
        &[],
        1,
    );
    assert_eq!(call.outcome.unwrap()[0].title, "200:ok");
    assert_eq!(call.report.requests_sent, 1);
    assert_eq!(network.requests().len(), 1);
    assert_eq!(
        network.requests()[0].0.url.as_str(),
        "https://api.example.org/radio"
    );

    // An undeclared host is refused, counted as refused, and never reaches the network.
    let call = host.radio(&plugin, &seed("fetch:https://evil.org/"), &[], 1);
    assert_eq!(call.outcome.unwrap()[0].title, "refused-host");
    assert_eq!(call.report.requests_sent, 0);
    assert_eq!(call.report.requests_refused, 1);
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn a_radio_answer_is_counted_against_the_output_ceiling() {
    let (host, plugin) = radio_plugin().unwrap();
    // `echo` tells the seed's title back, so a long title makes a long answer; the rest of the answer is 80 bytes plus
    // the element counts, so find the title that lands exactly on the ceiling from what the plugin returned.
    let measure = |len: usize| {
        let mut long = seed("echo");
        long.title = "a".repeat(len);
        host.radio(&plugin, &long, &[], 0)
    };
    let small = measure(1);
    let base = small.report.output.unwrap().get() - 1;
    let fits = measure(usize::try_from(MAX_OUTPUT - base).unwrap());
    assert!(fits.outcome.is_ok(), "{:?}", fits.outcome);
    assert_eq!(fits.report.output, Some(ByteSize::new(MAX_OUTPUT)));
    let over = measure(usize::try_from(MAX_OUTPUT - base + 1).unwrap());
    assert_eq!(
        over.outcome.unwrap_err(),
        PluginFailure::OutputTooLarge {
            size: ByteSize::new(MAX_OUTPUT + 1),
            limit: ByteSize::new(MAX_OUTPUT),
        }
    );
}

// Resolve (docs/plugin-host-spec.md, sections 3, 5, 6, 7 and 8): the `resolve` plugin's `resolve` does what the track's
// id says (see `fixtures/src/lib.rs`).

/// A host with the `resolve` plugin installed.
fn resolve_plugin() -> Result<(PluginHost, InstalledPlugin), String> {
    let host = host().map_err(|e| e.to_string())?;
    let plugin = host
        .install(&fixture("resolve")?)
        .map_err(|e| e.to_string())?;
    Ok((host, plugin))
}

#[test]
fn the_resolve_plugin_installs_with_the_resolve_capability_only() {
    let (_, plugin) = resolve_plugin().unwrap();
    assert_eq!(
        plugin
            .manifest()
            .capabilities()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [Capability::Resolve]
    );
}

#[test]
fn every_field_of_the_track_and_the_request_reaches_the_plugin_and_every_field_of_the_stream_comes_back()
 {
    let (host, plugin) = resolve_plugin().unwrap();
    let mut artist = ArtistRef::default();
    artist.id = "ar1".to_owned();
    artist.name = "Artist One".to_owned();
    let mut track = seed("echo");
    track.artists = vec![artist];
    track.duration = Some(Duration::from_millis(215_999));
    track.isrc = Some("FRZ039800212".to_owned());

    let playable = host
        .resolve(
            &plugin,
            &track,
            &["bad1".to_owned(), "bad2".to_owned()],
            Some("good1"),
        )
        .outcome
        .unwrap();
    assert_eq!(playable.url, "https://a.cdn.example.org/echo");
    assert_eq!(
        playable.headers,
        [
            (
                "x-track".to_owned(),
                "title=Seed;artists=ar1=Artist One;duration=Some(215999);isrc=Some(\"FRZ039800212\")"
                    .to_owned()
            ),
            (
                "x-request".to_owned(),
                "rejected=bad1,bad2;preferred=Some(\"good1\")".to_owned()
            ),
        ]
    );
    assert_eq!(playable.mime, None);
    assert_eq!(playable.valid_for, None);
    assert_eq!(playable.matched_id, "echoed");
}

#[test]
fn absent_optional_parts_of_the_request_arrive_absent() {
    let (host, plugin) = resolve_plugin().unwrap();
    let playable = host
        .resolve(&plugin, &seed("echo"), &[], None)
        .outcome
        .unwrap();
    assert_eq!(
        playable.headers[0].1,
        "title=Seed;artists=;duration=None;isrc=None"
    );
    assert_eq!(playable.headers[1].1, "rejected=;preferred=None");
}

#[test]
fn the_validity_arrives_as_a_duration_and_the_media_type_as_given() {
    let (host, plugin) = resolve_plugin().unwrap();
    let playable = host
        .resolve(&plugin, &seed("any-id"), &[], None)
        .outcome
        .unwrap();
    assert_eq!(playable.url, "https://a.cdn.example.org/stream");
    assert_eq!(playable.mime.as_deref(), Some("audio/mp4"));
    assert_eq!(playable.valid_for, Some(Duration::from_mins(359)));
    assert_eq!(playable.matched_id, "any-id");
    assert_eq!(playable.headers, Vec::<(String, String)>::new());
}

#[test]
fn each_error_of_the_resolve_plugin_comes_back_as_itself() {
    let (host, plugin) = resolve_plugin().unwrap();
    let cases = [
        ("error:not-found", PluginError::NotFound),
        ("error:unsupported", PluginError::Unsupported),
        (
            "error:rate-limited",
            PluginError::RateLimited {
                retry_after: Some(Duration::from_millis(1_500)),
            },
        ),
        (
            "error:invalid-input",
            PluginError::InvalidInput("bad track".to_owned()),
        ),
        (
            "error:upstream",
            PluginError::Upstream("service down".to_owned()),
        ),
    ];
    for (id, expected) in cases {
        let failure = host
            .resolve(&plugin, &seed(id), &[], None)
            .outcome
            .unwrap_err();
        assert_eq!(failure, PluginFailure::Plugin(expected), "{id}");
    }
}

#[test]
fn a_stream_on_a_declared_host_is_returned_as_the_plugin_gave_it() {
    let (host, plugin) = resolve_plugin().unwrap();
    for url in [
        "https://api.example.org/stream?id=1",
        "https://a.cdn.example.org/stream",
        "https://b.a.cdn.example.org/stream",
        // The host name is compared in lowercase; the URL is handed on untouched.
        "https://A.CDN.EXAMPLE.ORG/Stream",
    ] {
        let playable = host
            .resolve(&plugin, &seed(&format!("stream:{url}")), &[], None)
            .outcome
            .unwrap_or_else(|failure| panic!("{url}: {failure:?}"));
        assert_eq!(playable.url, url);
    }
}

#[test]
fn a_stream_on_an_undeclared_host_is_refused_with_the_host_it_named() {
    let (host, plugin) = resolve_plugin().unwrap();
    let cases = [
        ("https://evil.org/stream", "evil.org"),
        // `*.cdn.example.org` matches subdomains only, never the name itself.
        ("https://cdn.example.org/stream", "cdn.example.org"),
        (
            "https://api.example.org.evil.org/stream",
            "api.example.org.evil.org",
        ),
        ("https://EVIL.org/Stream", "evil.org"),
    ];
    for (url, named) in cases {
        let call = host.resolve(&plugin, &seed(&format!("stream:{url}")), &[], None);
        assert_eq!(
            call.outcome,
            Err(PluginFailure::InvalidResult(
                ResultFault::StreamHostUndeclared {
                    host: named.to_owned()
                }
            )),
            "{url}"
        );
        // The call ran: its report is kept with the failure.
        assert!(call.report.fuel_used.get() > 0, "{url}");
        assert!(call.report.output.is_some(), "{url}");
    }
}

#[test]
fn a_stream_that_is_not_an_https_url_on_a_host_name_is_refused() {
    let (host, plugin) = resolve_plugin().unwrap();
    for url in [
        "",
        "not a url",
        "http://a.cdn.example.org/stream",
        "//a.cdn.example.org/stream",
        "ftp://a.cdn.example.org/stream",
        "https://a.cdn.example.org:8443/stream",
        "https://user@a.cdn.example.org/stream",
        "https://user:secret@a.cdn.example.org/stream",
        "https://10.0.0.1/stream",
        "https://[::1]/stream",
        "https://a.cdn.example.org\\@evil.org/stream",
        "https://a.cdn.example.org/a b",
        "https://a.cdn.example.org/\u{e9}",
        "https:///stream",
    ] {
        let call = host.resolve(&plugin, &seed(&format!("stream:{url}")), &[], None);
        assert_eq!(
            call.outcome,
            Err(PluginFailure::InvalidResult(ResultFault::StreamUrl)),
            "{url:?}"
        );
    }
}

#[test]
fn a_stream_over_the_output_ceiling_fails_on_the_ceiling_before_the_host_check() {
    let (host, plugin) = resolve_plugin().unwrap();
    let url = format!("https://evil.org/{}", "a".repeat(70_000));
    let call = host.resolve(&plugin, &seed(&format!("stream:{url}")), &[], None);
    assert!(
        matches!(call.outcome, Err(PluginFailure::OutputTooLarge { .. })),
        "{:?}",
        call.outcome
    );
}

#[test]
fn a_plugin_that_does_not_provide_resolve_is_not_called_for_it_and_the_other_way_round() {
    let host = host().unwrap();
    let search_only = host.install(&fixture("good").unwrap()).unwrap();
    let call = host.resolve(&search_only, &seed("any-id"), &[], None);
    assert_eq!(
        call.outcome,
        Err(PluginFailure::NotProvided(Capability::Resolve))
    );
    assert_eq!(call.report, CallReport::default());

    let resolve_only = host.install(&fixture("resolve").unwrap()).unwrap();
    let call = host.search(&resolve_only, "daft", &[ItemKind::Track], None);
    assert_eq!(
        call.outcome.unwrap_err(),
        PluginFailure::NotProvided(Capability::Search)
    );
    assert_eq!(call.report, CallReport::default());
}

#[test]
fn a_resolve_call_reaches_the_network_only_through_the_sandbox() {
    let network = common::ok_network();
    let host = host_on(network.clone()).unwrap();
    let plugin = host.install(&fixture("resolve").unwrap()).unwrap();

    let call = host.resolve(
        &plugin,
        &seed("fetch:https://api.example.org/player"),
        &[],
        None,
    );
    assert_eq!(call.outcome.unwrap().matched_id, "200:ok");
    assert_eq!(call.report.requests_sent, 1);
    assert_eq!(
        network.requests()[0].0.url.as_str(),
        "https://api.example.org/player"
    );

    let call = host.resolve(&plugin, &seed("fetch:https://evil.org/"), &[], None);
    assert_eq!(call.outcome.unwrap().matched_id, "refused-host");
    assert_eq!(call.report.requests_sent, 0);
    assert_eq!(call.report.requests_refused, 1);
    assert_eq!(network.requests().len(), 1);
}

#[test]
fn a_stream_is_counted_against_the_output_ceiling() {
    let (host, plugin) = resolve_plugin().unwrap();
    // `echo` tells the title back in a header, so a long title makes a long answer; find the title that lands exactly
    // on the ceiling from what the plugin returned.
    let measure = |len: usize| {
        let mut long = seed("echo");
        long.title = "a".repeat(len);
        host.resolve(&plugin, &long, &[], None)
    };
    let small = measure(1);
    let base = small.report.output.unwrap().get() - 1;
    let fits = measure(usize::try_from(MAX_OUTPUT - base).unwrap());
    assert!(fits.outcome.is_ok(), "{:?}", fits.outcome);
    assert_eq!(fits.report.output, Some(ByteSize::new(MAX_OUTPUT)));
    let over = measure(usize::try_from(MAX_OUTPUT - base + 1).unwrap());
    assert_eq!(
        over.outcome.unwrap_err(),
        PluginFailure::OutputTooLarge {
            size: ByteSize::new(MAX_OUTPUT + 1),
            limit: ByteSize::new(MAX_OUTPUT),
        }
    );
}
