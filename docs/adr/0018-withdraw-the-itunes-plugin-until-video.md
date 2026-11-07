# 0018. Withdraw the `itunes` plugin until the video work starts

- Status: Accepted
- Date: 2026-11-07
- Deciders: the user

## Context

- [Plugin providers](../plugin-providers.md) (accepted 2026-11-06) planned `itunes` in `vehicoule-plugins` with
  `search`, `metadata`, `artwork` and `link-import`, second after `deezer` for `search` and `artwork`.
- What it adds for music, from that file's section 4 (checked 2026-11-06): a fallback when Deezer fails, the import
  of Apple Music links, and covers. What it lacks: ISRCs, artist pictures, playlists, an autocomplete endpoint; and its
  documentation limits it to "approximately 20 calls per minute (subject to change)".
- Its Search API also returns ebooks and music videos (same section), which makes it a candidate for the video and
  books domains, whose providers are not chosen yet.
- On 2026-11-07, when the music plugins were being written, the user said they did not know whether `itunes` would
  be useful, and decided to remove it and look at it again when the video work starts.

## Options

1. **Keep `itunes` for music**, built after `deezer`. Gives a fallback for search and artwork and the import of Apple
   Music links; costs a plugin to write, check and keep in the service's shape, for a service with a low rate limit.
2. **Decide after `deezer` is written**, on what Deezer was seen to miss. Defers the cost, keeps the question open.
3. **Withdraw `itunes` now**, and consider it again with the video domain. No plugin to maintain; `search` and
   `artwork` have no fallback, and Apple Music links are not imported.

## Decision

Option 3, the user's choice on 2026-11-07: `itunes` is not written for music. [Plugin providers](../plugin-providers.md)
loses it from every list; `deezer` alone provides `search`, `artwork` and `link-import`. Whether it comes back, for
video, books or music, is decided with the providers of the video domain.

## Consequences

- When `deezer` fails a search or an artwork request, the core has no other candidate: the failure reaches the user,
  who can try again later. The routing itself does not change (a priority list per capability, with one entry).
- A link to Apple Music is not imported; the core reports it as a link no plugin handles.
- `vehicoule-plugins` updates its own documents (AGENTS.md, README) in a pull request of its own, and its copy of
  `plugin-providers.md` with its next `scripts/sync-contract.sh update`.
- Section 4 of plugin providers keeps what was checked about iTunes on 2026-11-06, for the video work.
