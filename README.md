# notionless

Synchronisiert Seiten aus einer Notion-Datenbank als Markdown-Dokumente nach
[Paperless-ngx](https://docs.paperless-ngx.com/) — damit deine Notizen im selben
durchsuchbaren Archiv landen wie der Rest deiner Dokumente, statt in einem Cloud-Silo.

Läuft als Daemon: alle fünf Minuten (konfigurierbar) ein Abgleich, geänderte Seiten
werden in Paperless ersetzt, neue angelegt.

## Status

Ehrlicher Stand, damit niemand Zeit verliert:

- **Die Synchronisation ist einseitig: Notion → Paperless.** Erkennt der Sync, dass die
  Paperless-Seite neuer ist, wird das derzeit nur geloggt, nicht zurückgeschrieben.
- **Es werden nur `paragraph` und `heading_1..3` exportiert.** Listen, Code-Blöcke,
  To-dos, Tabellen und verschachtelte Blöcke fehlen im Markdown noch. Bei Seiten, die
  überwiegend daraus bestehen, landet entsprechend wenig in Paperless.
- Getestet gegen Paperless-ngx mit Notion-API-Version `2022-06-28`.

Beides steht auf der Roadmap. PRs sind willkommen.

## Wie Änderungen erkannt werden

Notion rundet `last_edited_time` auf Minuten, Zeitstempel allein sind also unzuverlässig.
notionless bildet stattdessen einen **SHA-256 über das exportierte Markdown** und legt ihn
am Paperless-Dokument ab. Gleicher Hash heißt: kein Upload, egal was die Zeitstempel sagen.
Der Zeitstempel entscheidet nur noch über die *Richtung* einer Änderung.

Zur Zuordnung schreibt notionless drei Custom-Fields an jedes Dokument:

| Feld | Inhalt |
| --- | --- |
| `notion_id` | Notion-Page-ID, der Anker zwischen beiden Systemen |
| `notion_last_edited` | `last_edited_time` aus Notion |
| `notion_content_hash` | SHA-256 des exportierten Markdowns |

**Die Felder musst du nicht anlegen.** Beim Start löst notionless sie über ihre Namen auf
und legt fehlende selbst an — die numerischen IDs sind pro Instanz verschieden und
interessieren dich nicht.

## Einrichtung

1. **Notion-Integration** unter https://www.notion.so/my-integrations anlegen, das
   *Internal Integration Secret* kopieren und die Integration in der Datenbank unter
   *Connections* freigeben. Ohne diesen Schritt liefert die API eine leere Liste.
   Die Datenbank braucht eine Titel-Spalte namens `Name`.
2. **Paperless-API-Token** unter *Einstellungen → Benutzer* erzeugen.
3. **Konfigurieren:**
   ```sh
   cp .env.example .env
   # .env ausfüllen
   ```
4. **Starten:**
   ```sh
   cargo run --release
   ```

Alle Einstellungen kommen aus Umgebungsvariablen, `.env` ist nur die bequeme lokale
Variante — im Container reichen normale env-Variablen.

## Konfiguration

| Variable | Pflicht | Bedeutung |
| --- | --- | --- |
| `PAPERLESS_URL` | ja | Basis-URL der Paperless-Instanz, `http://` im LAN ist ok |
| `PAPERLESS_TOKEN` | ja | Paperless-API-Token |
| `NOTION_URL` | ja | `https://api.notion.com/v1/databases/<DATABASE_ID>/query` |
| `NOTION_TOKEN` | ja | Notion Internal Integration Secret |
| `SYNC_INTERVAL_SECS` | nein | Sekunden zwischen zwei Durchläufen (Standard: `300`) |

## Wichtig zu wissen

Wird eine Seite in Notion geändert, **löscht** notionless das alte Paperless-Dokument
endgültig (inklusive Papierkorb — sonst greift Paperless' Duplikatsprüfung) und lädt die
neue Fassung hoch. Zusammen mit der oben genannten Block-Einschränkung heißt das: Inhalte,
die der Exporter nicht kennt, sind nach einem Update auch in Paperless weg. Wenn deine
Paperless-Dokumente die einzige Kopie sind, halte ein Backup bereit.

## Lizenz

MIT — siehe [LICENSE](LICENSE).
