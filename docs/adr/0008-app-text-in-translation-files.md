# 0008. Keep every text the app shows in translation files from the start

- Status: Proposed
- Date: 2026-11-06
- Deciders: the user

## Context

The app ships in French only for now. The user agreed on 2026-11-06 to keep its texts in translation files from the
first screen, so that translating later is adding a file, not finding every string in the code. The shell, the only
part that shows text, does not exist yet; this is the cheapest moment to set the rule.

- The core returns typed values and draws nothing ([ADR 0003](0003-core-architecture.md), AGENTS.md rule 4): it never
  holds a sentence, so only the shell translates.
- A plugin's name and version are shown as the plugin gives them, and its log messages are never shown as app text
  ([plugin host spec](../plugin-host-spec.md), sections 4 and 5): plugin text stays out of this system.
- Languages differ in plurals, grammatical gender and word order. A key-value file breaks on the first "1 titre /
  2 titres" unless something chooses the plural form.
- `fluent-bundle` 0.16.0 (crates.io, released 2025-05-22, `Apache-2.0 OR MIT`, repository `projectfluent/fluent-rs`)
  implements Project Fluent, Mozilla's format, with plural and select expressions. Its normal dependencies are
  `fluent-langneg`, `fluent-syntax`, `intl-memoizer`, `intl_pluralrules`, `rustc-hash`, `self_cell`, `smallvec` and
  `unic-langid` (crates.io dependency list of 0.16.0). Its build has not been tried in this workspace.
- `icu_plurals` 2.4.0 (ICU4X, crates.io, released 2026-10-22) gives the plural rules alone.

## Options

1. **Texts in the code, extracted later.** Nothing to set up now. Later, every string is found by hand, and the
   French sentences shape the code (concatenation, plurals in `if`).
2. **Fluent files, one per language, read with `fluent-bundle`.** Plurals, gender and variables handled by the
   format; translators know it. One dependency tree of eight crates, to pass `cargo deny`.
3. **Our own key-value files with plurals from `icu_plurals`.** Fewer crates; we write and test the parser, the
   variables and the plural selection ourselves.

## Decision

Recommended: option 2, Fluent.

- No text shown to the user is written in the shell's code: each one is a message in `fr.ftl`, read through a typed
  key, so a missing key does not compile.
- CI fails when a message is used by the code but missing from a language file, or present in a file but used by no
  code.
- Numbers, dates and durations are formatted by locale; the crate that does it is chosen with the shell, in its own
  pull request.
- The layout supports right-to-left languages from the first screen.

## Consequences

- The shell's first pull request brings `fluent-bundle`, the typed keys and the CI check; AGENTS.md gains the rule
  "no user-visible text in code" when this ADR is accepted.
- Adding a language is adding a file and passing the CI check, with no change to the code.
- Writing a screen costs a key per text, even in French only.
