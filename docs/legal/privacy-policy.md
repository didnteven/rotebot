# Macroscope privacy policy (draft)

Status: **draft for the owner and legal review, required before external testers (plan §8.3
P10).** It describes the code as of `m4/muse-findings-2` (2026-10-28). Do not publish it until
someone qualified has reviewed it. Product name, contact details and the effective date are
placeholders.

## The short version

Macroscope runs on your Mac. Your files, tools, results and history stay on your Mac in an
encrypted store. Macroscope has no account, no analytics and no crash reporting. It sends data
off your Mac only in the cases listed under "What leaves your Mac". Each of those is something
you set up or approve, and each one is recorded in the Privacy screen.

## What stays on your Mac

| Data | Where and how | How long |
|---|---|---|
| Tools you save (task description, generated program, settings) | Encrypted database (SQLCipher) in the app's data folder | Until you delete the tool |
| Run receipts (what was created or done, when, plan hash) | Encrypted database | Until you delete the tool |
| Staged outputs waiting for your approval | Encrypted staging files | Removed when you create or discard the output |
| Today entries from scheduled and manual runs | Encrypted database | Until you dismiss or delete them. The Today screen shows the last 7 days |
| Outbound records (what was sent where) | Encrypted database | Payload bodies 7 days, records 30 days |
| Connection detections from Show | Counts of which services were seen, no content | Until you delete them |
| Keys and tokens (Meta API key, connection tokens, OAuth client details) | macOS Keychain in release builds; a private file (owner-only permissions) in developer builds | Until you remove them or disconnect |

Files you create with a tool are written only to the folder you choose. Macroscope never
overwrites an existing file. Settings → Privacy can delete any category above, or everything.

## What leaves your Mac

1. **Building a tool with a model.** Macroscope sends your task description, column names and
   a small sample of rows to the model route you choose: Meta's API with your own key, or Muse
   Code with your own Muse plan sign-in. You see a preview before anything is sent. The
   offline demo route sends nothing. These services handle the data under their own terms.
2. **Model steps at run time**, only for tools that declare one. The save and schedule screens
   show the exact instruction and the data sent with it (for example, "today's events from
   Google Calendar"), and how often. Each call is limited in size and counted against a
   monthly cap.
3. **Web sources**, only for tools that declare them. These are HTTPS GET requests to sites you
   approve, for example `api.open-meteo.com`. No cookies, keys or personal data are added.
   Macroscope refuses web addresses that contain keys or tokens.
4. **Connections** (for example Gmail, Google Calendar, Notion, Linear). These are set up only
   when you choose to connect. Macroscope reads only the calls you approved when saving a tool.
   It sends, posts or changes something only after you approve that exact call. Sign-in uses
   the provider's own page, and tokens stay in the Keychain.
5. **Muse plan check.** If you use Muse Code, Macroscope reads your plan status and allowance
   with the Muse app already on your Mac. It never signs you in or out.

**Local only** in Settings blocks items 1–4. **Emergency stop** halts running work and pauses
schedules.

## Show (recording)

Show starts only when you start it, shows that it is recording, and stops after at most five
minutes. It notes which apps and window titles are in use so it can suggest connections, for
example "You used Gmail while recording". Matching happens inside the separate observer
process. Only service names reach the app. No screenshots, screen text, keystrokes or window
contents are stored or sent. Show needs macOS Accessibility permission, and you can revoke it
at any time.

## Things Macroscope does not do

- No analytics, usage tracking, advertising identifiers or crash reports.
- No selling or sharing of your data. Macroscope has no servers that receive it.
- Generated programs run in a sandbox with no network, no access to your keys or tokens, and
  access only to the input you selected.

## Open items before this can be published

- Legal review, the product name, a contact address and an effective date.
- The Meta and Muse terms (retention and training) must be summarized accurately for end users
  (plan §8.3). `store:false` is not a zero-retention promise.
- Decide whether Today entries should expire automatically. They currently stay until deleted.
- Re-check this document against the code before each release.
