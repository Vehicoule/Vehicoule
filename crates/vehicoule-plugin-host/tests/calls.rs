//! Capability calls against the test plugins of `fixtures/` (docs/plugin-host-spec.md, sections 3, 6, 7 and 9).
//!
//! The `good` plugin's `search` does what its query says (see `fixtures/src/lib.rs`), so one plugin covers every
//! behavior a call can have.

mod common;

use std::time::Duration;

use common::{MAX_OUTPUT, fixture, host};
use vehicoule_plugin_host::{
    ByteSize, Capability, InstalledPlugin, Item, ItemKind, PluginError, PluginFailure, PluginHost,
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
        .unwrap();
    // The plugin answers `next` = cursor + "+": it saw the cursor.
    assert_eq!(page.next.as_deref(), Some("page-2+"));
    assert_eq!(page.items.len(), 1);
    assert!(
        matches!(&page.items[0], Item::Playlist(p) if p.title == "daft" && p.owner.is_none()),
        "{:?}",
        page.items
    );
    let empty = host.search(&plugin, "daft", &[], None).unwrap();
    assert!(empty.items.is_empty(), "{:?}", empty.items);
}

#[test]
fn suggestions_come_back_in_the_plugin_order() {
    let (host, plugin) = good().unwrap();
    assert_eq!(
        host.suggest(&plugin, "daft").unwrap(),
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
            .unwrap_err();
        assert_eq!(failure, PluginFailure::Plugin(expected), "{query}");
    }
}

#[test]
fn an_endless_call_is_stopped_by_fuel() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "loop", &[], None).unwrap_err();
    assert_eq!(failure, PluginFailure::FuelExhausted);
}

#[test]
fn a_call_that_grows_memory_without_bound_is_stopped_at_the_ceiling() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "grow", &[], None).unwrap_err();
    assert_eq!(failure, PluginFailure::MemoryLimit);
}

#[test]
fn a_trap_fails_the_call_and_the_next_call_works() {
    let (host, plugin) = good().unwrap();
    let failure = host.search(&plugin, "trap", &[], None).unwrap_err();
    assert!(matches!(failure, PluginFailure::Trap(_)), "{failure:?}");
    let page = host
        .search(&plugin, "after", &[ItemKind::Track], None)
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
    assert!(host.search(&plugin, "loop", &[], None).is_err());
    assert!(host.search(&plugin, "grow", &[], None).is_err());
    assert!(
        host.search(&plugin, "daft", &[ItemKind::Track], None)
            .is_ok()
    );
}

#[test]
fn nothing_a_plugin_keeps_in_memory_survives_to_the_next_call() {
    // The plugin counts its calls in a static: in a fresh instance per call, it always counts one.
    let (host, plugin) = good().unwrap();
    for _ in 0..3 {
        let page = host.search(&plugin, "count", &[], None).unwrap();
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
            .unwrap_err(),
        PluginFailure::NotProvided(Capability::Search)
    );
    assert_eq!(
        host.suggest(&plugin, "daft").unwrap_err(),
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
        .unwrap();
    assert_eq!(page.items.len(), usize::try_from(fits).unwrap());
    let failure = host
        .search(&plugin, &format!("empty:{}", fits + 1), &[], None)
        .unwrap_err();
    assert_eq!(
        failure,
        PluginFailure::OutputTooLarge {
            size: ByteSize::new(MAX_OUTPUT + 1),
            limit: ByteSize::new(MAX_OUTPUT),
        }
    );
}
