<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <img src="assets/logo-light.svg" alt="notionless" width="310">
</picture>

Your Notion notes are stuck in Notion. Everything else you own (invoices, contracts,
scanned mail, manuals) probably already lives in [Paperless-ngx](https://docs.paperless-ngx.com/),
full-text searchable, tagged, backed up, self-hosted. Your notes are the one thing that
isn't in there. Search your archive for something and Notion just doesn't show up,
because it's a different app on a different server you don't control.

notionless closes that gap: it runs as a small daemon, watches a Notion database, and
mirrors every page into Paperless as a Markdown document, automatically, on a schedule,
with no manual export. Once it's running, "search everything I own" actually means
everything, notes included.

It's not a one-off export script. It diffs on every cycle, so only pages that actually
changed get re-uploaded, and it's built to survive being pointed at a Paperless instance
that already has your documents in it, see [duplicate handling](#how-changes-are-detected)
below.

## Why this exists

Notion is a great place to *write*. It's a bad place to *keep* things:

- No real full-text search across notes and everything else you've archived.
- Your notes live behind Notion's uptime, Notion's pricing, and Notion's export
  formats, not yours.
- If you're already running Paperless for the rest of your paperwork, your notes are
  the one category that's still siloed off.

I'm building this because I'm starting a new degree soon and want to take notes in
Notion without ending up with a second, unsearchable archive next to Paperless. This
needs to work reliably before day one, not eventually.

If you don't self-host anything, this project isn't for you. If you already run
Paperless-ngx and want your Notion notes to show up in the same search, this is a
five-minute setup, not a migration project.

## Status

Honest state, so nobody wastes time:

- **The sync is one-way: Notion → Paperless.** If the sync detects the Paperless side is
  newer, that's currently only logged, not written back.
- **Only `paragraph` and `heading_1..3` are exported.** Lists, code blocks, to-dos,
  tables, and nested blocks are still missing from the Markdown. Pages that consist
  mostly of those end up with very little content in Paperless.
- Tested against Paperless-ngx with Notion API version `2022-06-28`.

Both are on the roadmap. PRs welcome: this is a small, readable Rust codebase
(see [Project layout](#project-layout)), not a framework to learn first.

## How changes are detected

Notion rounds `last_edited_time` to the minute, so timestamps alone aren't reliable.
notionless instead computes a **SHA-256 of the exported Markdown** and stores it on the
Paperless document. Matching hashes mean: no upload, no matter what the timestamps say.
The timestamp only decides the *direction* of a change.

To keep the two sides linked, notionless writes three custom fields to every document:

| Field | Content |
| --- | --- |
| `notion_id` | Notion page ID, the anchor between both systems |
| `notion_last_edited` | `last_edited_time` from Notion |
| `notion_content_hash` | SHA-256 of the exported Markdown |

**You don't need to create these fields.** notionless resolves them by name at startup
and creates any that are missing: the numeric IDs differ per instance and don't matter
to you.

**If a document with identical content already exists in Paperless** (e.g. because you
imported it manually before using notionless), Paperless rejects the upload as a
duplicate. In that case notionless automatically links the existing document to the
Notion page instead of re-uploading it (and getting rejected again) on every cycle.
This is safe because Paperless' own duplicate detection is based on a byte hash of the
file, so the adoption only kicks in once the content is already confirmed to match.
In other words: you can point notionless at a Paperless instance you already use, and
it adopts what's already there instead of fighting it.

## Setup

1. **Create a Notion integration** at https://www.notion.so/my-integrations, copy the
   *Internal Integration Secret*, and share the database with the integration under
   *Connections*. Without this step the API returns an empty list. The database needs a
   title column named `Name`.

   To find the database ID for `NOTION_URL`, open the database as a full page in
   Notion and look at the URL: `https://www.notion.so/<workspace>/<DATABASE_ID>?v=...`.
   `DATABASE_ID` is the 32-character string right before `?v=` (dashes optional, the
   API accepts it either way).
2. **Create a Paperless API token.** Log in to the Django admin panel at
   `<PAPERLESS_URL>/admin/authtoken/tokenproxy/` (requires an admin/superuser account),
   add a token, and pick your user.
3. **Configure:**
   ```sh
   cp .env.example .env
   # fill in .env
   ```
4. **Run:**
   ```sh
   cargo run --release
   ```

All settings come from environment variables; `.env` is just the convenient local
option, in a container plain env vars are enough.

## Configuration

| Variable | Required | Meaning |
| --- | --- | --- |
| `PAPERLESS_URL` | yes | Base URL of the Paperless instance; `http://` on a LAN is fine |
| `PAPERLESS_TOKEN` | yes | Paperless API token |
| `NOTION_URL` | yes | `https://api.notion.com/v1/databases/<DATABASE_ID>/query` |
| `NOTION_TOKEN` | yes | Notion Internal Integration Secret |
| `SYNC_INTERVAL_SECS` | no | Seconds between sync cycles (default: `300`) |

## Important to know

When a page changes in Notion, notionless **permanently deletes** the old Paperless
document (trash included, otherwise Paperless' duplicate check gets in the way) and
uploads the new version. Combined with the block limitation above, that means: content
the exporter doesn't understand is gone from Paperless after an update too. If your
Paperless documents are the only copy, keep a backup.

## Project layout

- `src/main.rs`: entry point, env config, startup, the sync loop.
- `src/paperless.rs` + `src/paperless/model.rs`: everything that talks to Paperless.
- `src/notion.rs` + `src/notion/model.rs`: everything that talks to Notion.
- `src/sync.rs`: the diffing logic (what changed, in which direction) and one sync
  cycle that ties Notion and Paperless together.

## License

MIT, see [LICENSE](LICENSE).
