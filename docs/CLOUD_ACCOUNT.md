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

## Student Mode progress

Student Mode saves progress to `student/progress.json` in the COVALOS config folder first, and merges it with these tables when an account is signed in: lessons keep the best score, the most attempts and the most advanced status; profile fields come from the newer copy. Only metrics, lesson state and review cards are uploaded. Exercise code, file paths, chat and the per-session score history stay on the computer.

```sql
create table public.student_profiles (
  user_id uuid primary key references auth.users on delete cascade,
  active_track text,
  streak_days int not null default 0,
  longest_streak int not null default 0,
  last_study_date date,
  daily_goal_minutes int not null default 20,
  xp_total int not null default 0,
  updated_at timestamptz not null default now()
);
create table public.student_lesson_progress (
  user_id uuid not null references auth.users on delete cascade,
  track_id text not null,
  lesson_id text not null,
  status text not null check (status in ('locked', 'in_progress', 'completed')),
  best_score smallint not null default 0 check (best_score between 0 and 100),
  attempts int not null default 0,
  hints_used int not null default 0,
  completed_at date,
  updated_at timestamptz not null default now(),
  primary key (user_id, track_id, lesson_id)
);
create table public.student_review_queue (
  user_id uuid not null references auth.users on delete cascade,
  track_id text not null,
  lesson_id text not null,
  concept text not null,
  next_review_at date not null,
  interval_days int not null default 1,
  updated_at timestamptz not null default now(),
  primary key (user_id, track_id, lesson_id)
);
alter table public.student_profiles enable row level security;
alter table public.student_lesson_progress enable row level security;
alter table public.student_review_queue enable row level security;
create policy "own profile" on public.student_profiles
  for all using (auth.uid() = user_id) with check (auth.uid() = user_id);
create policy "own lessons" on public.student_lesson_progress
  for all using (auth.uid() = user_id) with check (auth.uid() = user_id);
create policy "own reviews" on public.student_review_queue
  for all using (auth.uid() = user_id) with check (auth.uid() = user_id);
```

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
