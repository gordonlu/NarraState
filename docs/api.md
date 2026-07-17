# HTTP API

The local API prefix is `/api/v1`. JSON errors use `application/problem+json` with `type`, `title`, `status`, and `detail`. No authentication is provided in v0.1 because the service is intended for local use.

## Cases and configuration

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Service health and version |
| GET | `/config/public` | Masked provider configuration state |
| POST | `/config/test-provider` | Test and save non-secret provider settings |
| GET | `/cases` | Public case summaries |
| GET | `/cases/{case_id}` | Redacted public case detail |
| POST | `/cases/{case_id}/visuals/generate` | Append missing or regenerate decorative visuals for a generated case |
| POST | `/cases/validate` | Validate a submitted case definition |
| POST | `/cases/install` | Validate and atomically install inline Manifest/Template content |
| POST | `/games` | Create an immutable v0.2 case instance and player session |

The provider API key is accepted for the connectivity request or read from the server environment. It is never returned or persisted.

`POST /cases/install` accepts `manifest`, `template`, and an optional `generation_report` JSON object. It never accepts a server filesystem path. Inline v0.2 installation currently rejects asset-bearing packages; asset archives will use a separately bounded upload format. The server writes into `NARRASTATE_CASE_INSTALL_DIR` (default `data/installed-cases`), validates from a temporary directory, then atomically renames the completed package.

Visual generation accepts `{"mode":"append_missing"}` or `{"mode":"regenerate_all"}`. It uses only the independently configured image Provider. The response reports attempted, updated, failed, and total image counts. `visual_status.failure_code` provides a stable category and `failure_detail` may contain a bounded, redacted structured message from the Provider. Failed replacements retain their previous images; the endpoint never changes case truth or frozen sessions.

## Games and truth selection

```http
POST /api/v1/games
Content-Type: application/json

{
  "case_id":"rain-gallery-variants",
  "variant_selection":{"mode":"random"},
  "seed":928341,
  "mode":"mock"
}
```

`variant_selection.mode` is `default`, `random`, or—only when developer mode is explicitly enabled with `NARRASTATE_DEVELOPER_MODE=1`—`specific`. A specific request also includes `variant_id`. Random selection considers only enabled variants that passed compilation, validation, and deterministic simulation. The same case version and seed produce the same selection. AI-generated cases use random selection in the normal player UI; their first variant remains a schema-level default for compatibility, not an author recommendation.

The response returns `session_id`, `instance_id`, `case_id`, `case_version`, and `seed`. It deliberately omits the selected variant, responsible character, hidden facts, and variant title. The complete compiled case is persisted before Session creation, and subsequent actions load that immutable snapshot.

## Sessions

Create a session:

```http
POST /api/v1/sessions
Content-Type: application/json

{"case_id":"rain-gallery","mode":"mock","target_character_id":"luo-cheng"}
```

`mode` is `mock` or `llm`. Session responses include only discovered facts and evidence, public dialogue, accusations, status, and `revision`.

| Method | Path | Purpose |
|---|---|---|
| POST | `/sessions` | Compatibility endpoint; creates a frozen single-variant instance internally |
| GET | `/sessions/{id}` | Recover current public state |
| GET | `/sessions/{id}/events` | Read redacted event metadata |
| POST | `/sessions/{id}/actions` | Submit a turn and receive SSE |
| POST | `/sessions/{id}/accusations` | Submit an evidence-backed accusation |
| POST | `/sessions/{id}/restart` | Create a fresh session for the same case |
| GET | `/sessions/{id}/conclusion` | Read the report after resolution |
| GET | `/sessions/{id}/debug` | Read explicit spoiler/debug state |

## Action stream

```http
POST /api/v1/sessions/{id}/actions
Content-Type: application/json
Accept: text/event-stream

{
  "client_action_id":"4c166025-8f52-4af6-91ba-4b6d783df68a",
  "expected_revision":0,
  "target_character_id":"luo-cheng",
  "text":"门禁记录怎么解释？",
  "attached_evidence_ids":["ev_card_log"]
}
```

Successful streams emit, in order: `turn.accepted`, `turn.progress`, `dialogue.delta`, `state.public_changed`, and `turn.completed`. A retry with the same `client_action_id` returns the committed result without creating a second turn. A stale distinct action returns HTTP 409; reload the session and resubmit intentionally.

`turn.completed` contains `session_id`, `turn_id`, `revision`, `utterance`, and `degraded`. When `degraded` is true, a safe fallback was used and the committed state remains authoritative.

## Redaction boundary

Normal endpoints never include culprit labels, hidden facts, internal numeric state, defenses, unrevealed disclosures, prompts, or API keys. The debug endpoint intentionally crosses part of that boundary and must remain behind the UI spoiler warning.
