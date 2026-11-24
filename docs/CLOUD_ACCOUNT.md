# COVALOS account (Supabase)

COVALOS can sign in to a COVALOS cloud account to sync settings and to be reached from the phone app. Everything is opt-in and off by default.

## Server address

Settings → COVALOS account → Server takes a server address and its public anon key. When those are empty, COVALOS reads:

- `COVALOS_SUPABASE_URL`, for example `https://<project>.supabase.co`, or `http://127.0.0.1:54321` for a local Supabase
- `COVALOS_SUPABASE_ANON_KEY`

COVALOS only accepts `https://`. Plain `http://` is allowed only for `localhost`, `127.0.0.1` and `*.localhost`.

## Sign-in

COVALOS uses OAuth with PKCE (S256) in the browser and gets the redirect on a loopback listener at `http://127.0.0.1:<random port>/callback`. Add `http://127.0.0.1:*/callback` to the project's allowed redirect URLs, and turn on the providers shown in Settings (GitHub, Google, Discord).

The refresh token goes to Windows Credential Manager under `covalos:covalos/cloud/<host>/refresh-token`. The access token only lives in memory. `preferences.json` never holds either one. Other systems have no supported store yet, so on those COVALOS keeps the session in memory until it closes.

## Cloud Config Sync

Table:

```sql
create table public.editor_configs (
  user_id uuid primary key references auth.users on delete cascade,
  payload jsonb not null,
  updated_at timestamptz not null default now()
);
alter table public.editor_configs enable row level security;
create policy "own config" on public.editor_configs
  for all using (auth.uid() = user_id) with check (auth.uid() = user_id);
```

`payload` holds theme, syntax colors, language, fonts, density, zoom, motion, pinned and favorite chats, `agent-rules.md` and skills. COVALOS never uploads workspace paths, recent folders, notes, tasks, open tabs, provider accounts or API keys. If agent rules or a skill look like they contain a secret, COVALOS leaves them out of the upload.

## Phone bridge

COVALOS joins the private Realtime channel `covalos-remote:<user id>`. Limit it to its owner:

```sql
create policy "own remote channel" on realtime.messages
  for all to authenticated
  using (realtime.topic() = 'covalos-remote:' || auth.uid()::text)
  with check (realtime.topic() = 'covalos-remote:' || auth.uid()::text);
```

Broadcast events:

- `state` (from COVALOS): sessions (id, title, status, provider, model), pending approvals (file names only, never full paths), and recent Luma lines. Text is redacted the same way as the audit log.
- `command` (from the phone): `{id, sent_ms, kind, ...}` where `kind` is `luma_order` (`text`), `chat_prompt` (`session`, `text`), `approval` (`ticket`, `allow`) or `refresh`. COVALOS ignores commands more than two minutes old.
- `result` (from COVALOS): `{id, ok, message}`.

Phone commands follow the same rules as local input. Luma keeps its effort ceiling. Agent tool calls still go through the agent gate. The phone can always deny a pending request. It can allow one only when "Let the phone allow pending requests" is on and the action is not marked sensitive, and COVALOS never remembers a grant made from the phone.
