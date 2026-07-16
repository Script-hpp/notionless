# notionless

Syncs pages from a Notion database as Markdown documents into
[Paperless-ngx](https://docs.paperless-ngx.com/) — so your notes end up in the same
searchable archive as the rest of your documents, instead of a cloud silo.

Runs as a daemon: a sync every five minutes (configurable), changed pages are replaced
in Paperless, new ones are created.

## Status

Honest state, so nobody wastes time:

- **The sync is one-way: Notion → Paperless.** If the sync detects the Paperless side is
  newer, that's currently only logged, not written back.
- **Only `paragraph` and `heading_1..3` are exported.** Lists, code blocks, to-dos,
  tables, and nested blocks are still missing from the Markdown. Pages that consist
  mostly of those end up with very little content in Paperless.
- Tested against Paperless-ngx with Notion API version `2022-06-28`.

Both are on the roadmap. PRs welcome.

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
and creates any that are missing — the numeric IDs differ per instance and don't matter
to you.

**If a document with identical content already exists in Paperless** (e.g. because you
imported it manually before using notionless), Paperless rejects the upload as a
duplicate. In that case notionless automatically links the existing document to the
Notion page instead of re-uploading it — and getting rejected again — on every cycle.
This is safe because Paperless' own duplicate detection is based on a byte hash of the
file, so the adoption only kicks in once the content is already confirmed to match.

## Setup

1. **Create a Notion integration** at https://www.notion.so/my-integrations, copy the
   *Internal Integration Secret*, and share the database with the integration under
   *Connections*. Without this step the API returns an empty list. The database needs a
   title column named `Name`.
2. **Create a Paperless API token** under *Settings → Users*.
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
option — in a container, plain env vars are enough.

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
document (trash included — otherwise Paperless' duplicate check gets in the way) and
uploads the new version. Combined with the block limitation above, that means: content
the exporter doesn't understand is gone from Paperless after an update too. If your
Paperless documents are the only copy, keep a backup.

## Project layout

- `src/main.rs` — entry point: env config, startup, the sync loop.
- `src/paperless.rs` + `src/paperless/model.rs` — everything that talks to Paperless.
- `src/notion.rs` + `src/notion/model.rs` — everything that talks to Notion.
- `src/sync.rs` — the diffing logic (what changed, in which direction) and one sync
  cycle that ties Notion and Paperless together.

## License

MIT — see [LICENSE](LICENSE).
