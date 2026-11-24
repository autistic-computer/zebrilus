---
type: Design Plan
title: Agent accounts by swapping the CLI's own login
description: Claude Code and Codex accounts stop being per-account config folders. Each CLI keeps its one native login and its one native folder; zz parks the logins that are not in use and swaps the chosen one into the native store when no chat of that provider runs. zz remembers who each login is from its own sign-in, never from the CLI's profile cache.
status: built
timestamp: 2026-11-23T15:42:13-03:00
---

# Why

Per-account folders (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) split the user's state: each folder had
its own `.claude.json` (tool approvals, folder trust, MCP toggles, onboarding), and whatever the
link list missed (`ide`, `jobs`, `chrome`, anything a later CLI adds) behaved differently in zz.
Claude Code or Codex opened outside zz never saw any of it.

The label was also wrong: the email came from `oauthAccount` in `.claude.json`, which any process
sharing that folder rewrites. The Claude desktop app runs Claude Code on `~/.claude` with its own
account, so on 2026-11-23 the terminal login held one account's token under another account's
email, and zz showed the second account's name over the first one's usage.

# The model

- A **slot** is one login of one provider (Claude Code or Codex), named `a1`, `a2`, ... as before.
  Antigravity, Grok and Cursor keep their single login and have no slots.
- Exactly one slot per provider is **active**: its login sits in the CLI's native store. Every
  chat of that provider runs on it, with no `CLAUDE_CONFIG_DIR` or `CODEX_HOME` of zz's. So do
  Claude Code and Codex opened anywhere else on the machine.
- The other slots are **parked**: zz holds their login, and nothing else uses it, so nobody else
  renews and invalidates it.
- Native stores: Claude Code on macOS is the Keychain item `Claude Code-credentials` (with
  `-<first 8 hex of sha256 of CLAUDE_CONFIG_DIR>` when the daemon has that variable), account the
  user name, written the way the CLI writes it (`security -i` with `add-generic-password -U -a .. -s
  .. -X <hex>` on stdin, so the secret never reaches an argument list; a line over 4094 bytes, more
  than `security -i` reads at once, goes in as arguments). Claude Code on Linux is
  `<config>/.credentials.json`. Codex is `<CODEX_HOME or ~/.codex>/auth.json` (the file store;
  a keyring store is refused).
- Parked store: macOS Keychain items of service `zz-agent-login`, account
  `<data dir>|<provider>|<slot>`; Linux files `<data>/zz/accounts/parked/<provider>/<slot>`, 0600.
- The record `<data>/zz/accounts/logins.json` (0600) lists each provider's slots with the email,
  organization and plan zz read when the slot was signed in or adopted, and which slot is active.
  It holds no secret.

# Who a login is

- A sign-in through zz names its login: the probe right after the CLI stores it reads the profile
  the CLI just fetched.
- Codex's `account/read` derives the account from `auth.json` itself, so Codex identities are
  checked on every refresh, and a login someone else put in `auth.json` is adopted as its own slot.
- Claude Code has no such check: `claude auth status`, `initialize` and `get_status` all read the
  cached profile. After adoption zz never reads a Claude identity from the cache again, so a login
  changed outside zz keeps the active slot's name until the user signs in through zz.
- A sign-in whose identity matches an existing slot reuses that slot (its old login is dropped),
  so signing in again is how a wrong name gets fixed.

# Swapping

`agent-account -t <pane> <slot>` now moves the machine's login of the pane's provider, not one
pane. Busy rules are the old ones over every chat of that provider: refused with `busy:` while one
runs, `--now` moves anyway, `--when-idle` waits until all are idle. The swap:

1. Settle the record as a listing does, so a login zz has not recorded becomes a slot first.
   Close every chat of the provider and wait for its process to exit (`close_and_wait`).
2. Read the native login and park it under the active slot (the latest renewed copy).
3. Write the chosen slot's parked login into the native store and read it back.
4. Delete the parked copy, mark the slot active, set every chat's descriptor to it.
5. Codex: `codex app-server daemon restart`, when its control socket exists, so the shared app
   server reads the new `auth.json`.
6. Reopen the chats on their sessions.

A failed write puts the old login back. A Claude Code or Codex the user runs outside zz during a
swap keeps its old token in memory and may write a renewed copy back afterwards; zz cannot stop
that.

# Signing in

- The CLI's login runs against the native store, with no account variable.
- Before it starts, the active login is parked (copied). If the sign-in fails or is cancelled the
  copy is dropped, since the native store is untouched.
- When it succeeds, the probe names the login and picks the slot (an existing one with that
  identity, the one asked for with `--account`, else the next free `aN`). If any chat of the
  provider is busy, the new login is parked, the previous one goes back into the native store, and
  the card offers Switch when idle and Switch now as before. Otherwise the new slot becomes active
  and every chat of the provider reopens on it.
- The terminal window path (clients without `agent-login-v1`, `--terminal`) runs the CLI's login
  on the native store and does no slot bookkeeping.

# Adoption and migration

On the first account refresh of a provider with no record:

- Each folder of the old design under `<data>/zz/accounts/<provider>/<id>` becomes a parked slot
  with the same id: its login is read from its own Keychain item or file, its name from its own
  `.claude.json` or from `account/read` with `CODEX_HOME` set to it. The folders are left in place.
- The native login, when signed in, becomes the active slot under the next free id, named by the
  probe (for Claude Code, from the cache, once).
- Every chat of the provider gets the active slot as its descriptor's account.

# Not built

Removing a slot, signing a slot out, and noticing that a Claude Code login changed outside zz.
