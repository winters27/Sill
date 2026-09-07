# The update endpoint

Sill asks `https://sill.winters.app/latest.json` for the update manifest. This
worker answers it by fetching GitHub and passing the answer through unchanged,
and counts the check on the way.

It exists to answer one question: **roughly how many machines run Sill.**

## What it stores

One row per machine per day: the day, and `sha256(address + secret + day)` cut
to sixteen characters.

The day is inside the hash, so the same machine gets a different identifier
tomorrow. Two rows from two days cannot be shown to be the same machine, which
means this can count how many checked in on a day and can never follow one
across days. No address is written down at any point, and there is no table
mapping an identifier back to one.

Sill sends no identifier of its own. The only client change is which host it
asks.

## What it deliberately does not do

- It holds no copy of `latest.json`, so there is nothing to keep in sync and
  nothing that can go stale. GitHub remains the source of truth.
- It does not rewrite the manifest. The updater checks a minisign signature
  against a key compiled into Sill, so anything altered here would fail on
  every machine at once.
- It does not serve the installer. The manifest points at GitHub for that, so
  the download never comes through here.
- It cannot break an update. Every failure path still returns the proxied
  answer, the counting happens after the response is sent, and
  `src-tauri/tauri.conf.json` lists GitHub as a second endpoint.

## Setting it up

```bash
npm install
npx wrangler d1 create sill-updates
```

Put the id it prints into `database_id` in `wrangler.jsonc`, then:

```bash
npm run schema
npx wrangler secret put SALT
npm run deploy
```

`SALT` is any long random string. It never leaves Cloudflare, and rotating it
only means the identifiers change shape from that day, which costs nothing.

## Reading the number

```bash
npm run count
```

Raw check counts, which are dozens per machine per day, are in the Cloudflare
dashboard for the worker and need nothing from here.

## What the number is worth

An estimate, not a census. Everybody behind one office or household address
reads as one machine; somebody on mobile data over a week reads as several.
Only installs from 0.2.0 onward are counted at all, because the endpoint is
compiled in and every earlier release asks GitHub directly. Say "about this
many" and it will be true.
