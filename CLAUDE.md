# notionless

One-way sync daemon: Notion database pages -> Markdown documents in Paperless-ngx.

## Language

All code, comments, doc comments, commit messages, `println!` output, and
documentation (README, `.env.example`) are in **English**. This is an open-source
project; German only in direct conversation with the maintainer, never in anything
committed.

Never use the em dash (`—`) anywhere in this repo, in prose or in code: not in
comments, doc comments, README, commit messages, or log output. Use a comma, colon,
period, or parentheses instead. Same goes for chat replies to the maintainer.

## Module layout

Each module owns one external system or one concern, don't let that blur back
together as the codebase grows:

- `src/main.rs`: entry point only, env loading, client setup, the outer sync loop.
  No business logic here.
- `src/paperless.rs` + `src/paperless/model.rs`: all Paperless-ngx API interaction
  (custom fields, document upload/delete, duplicate handling). Nothing here knows
  about Notion.
- `src/notion.rs` + `src/notion/model.rs`: all Notion API interaction (page listing,
  content export). Nothing here knows about Paperless.
- `src/sync.rs`: the diffing logic and `run_sync_cycle`, the only place that imports
  both `paperless` and `notion`.

When adding a new external call, put it in the module for that system, not in
`sync.rs` or `main.rs`. When adding sync/diffing logic, it goes in `sync.rs`, not
scattered into `paperless.rs`.

Prefer a small named struct over a growing tuple or parameter list once either carries
more than two or three pieces of related data (see `paperless::DocumentRecord`,
`paperless::NotionPageRef`, `sync::ExportedPage`). That's what keeps `HashMap<String,
(i64, String, String)>`-style signatures from creeping back in.

## Comments

Only comment the **why**, never the what: the code already says what it does.
Good candidates: a non-obvious API quirk (see the `normalize_next_url` doc comment for
why Paperless' `next` URL can't be followed as-is), an invariant that would silently
break if violated (e.g. "always re-fetch both sides every cycle"), a safety argument
for something that looks risky (see `adopt_existing`'s doc comment on why merging
custom fields matters).

Bad candidates: restating a loop ("iterate over all entries"), restating a variable
name, or explaining what a struct's fields are when the field names already say it.
If you'd delete a comment and nothing about the code becomes less clear, don't add it
in the first place.

## Testing

Unit tests live in `#[cfg(test)] mod tests` at the bottom of the file they test
(see `src/paperless.rs`). They cover pure logic (URL rewriting, error-message parsing),
nothing that requires a live Paperless/Notion instance.

Before trusting a behavioral fix (not just a compile fix), verify it against a real
Paperless instance if one is available in the session, not just `cargo build`/
`cargo test`. Several bugs in this project only showed up at runtime against live data
(the reverse-proxy pagination 401, the duplicate-upload loop); clippy and the type
checker did not and could not catch either.

## Before committing

Run `cargo build`, `cargo test`, and `cargo clippy --all-targets`. The project should
have zero warnings, not just zero errors.
