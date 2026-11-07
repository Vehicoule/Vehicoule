# 0017. Send nothing to the project, except a crash report the user chooses to send

- Status: Accepted
- Date: 2026-11-07
- Deciders: the user

## Context

Vehicoule has no server of its own: content comes from plugins, and each plugin talks to its provider
([ADR 0007](0007-plugin-host.md)). Whether the app also sends anything to the project is a product commitment that
users, stores and contributors read, so it is decided once, here.

- A crash can only be fixed once someone sees it. The runner already keeps a bounded log of every event and command
  that a test can replay ([core playback spec §6](../core-playback-spec.md)), so one report can be enough to reproduce
  a bug.
- That log holds what the user played and searched for: personal data the user must see before it leaves the device.
- Google Play asks every app to fill in its Data safety section and to give a privacy policy, even an app that
  collects nothing ([Play Console Help](https://support.google.com/googleplay/android-developer/answer/10787469)).

## Options

1. **Nothing, ever.** No telemetry and no crash reports. The strongest promise; crashes are known only when a user
   writes an issue by hand.
2. **No telemetry; crash reports only on request.** Nothing is sent automatically. After a crash, the user may send a
   report, after seeing all of its content. Crashes become fixable from their log without tracking anyone.
3. **Telemetry on by default, with a way to opt out.** Usage statistics and automatic crash reports. Breaks the
   promise of an app with no server, and needs a server and its data to protect.

## Decision

Option 2. The user chose it on 2026-11-07.

- The app sends the project nothing on its own: no usage statistics, no analytics, no identifier, no automatic crash
  report.
- After a crash, the app may offer to send a report. The user sees the whole report before sending and can decline;
  declining changes nothing else in the app.
- How a report reaches the project (a prefilled issue, an email, a server of ours) and what exactly it holds are
  decided by a later ADR, before the feature ships, with any dependency it needs.

## Consequences

- `docs/privacy.md` states this promise, then lists what the app keeps on the device and what each provider receives
  through its plugin. The public privacy policy that Google Play asks for comes from that document.
- A crash report is built and sent by the shell, at the edge; the core stays without I/O
  ([ADR 0003](0003-core-architecture.md)).
- Adding any data sent to the project without the user's action needs a new ADR that supersedes this one.
