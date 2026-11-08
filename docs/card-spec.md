# tapbox card format — draft v0.1

Status: **draft**, open for change until v1.0. The goal is that any card written to this spec plays on any compatible box.

The key words MUST, SHOULD and MAY are used as in RFC 2119.

## 1. Principles

- A card is a **pointer**, not storage. Audio never lives on the card.
- Content is identified by **hash** where possible, so a box can find it in its own library, offline, no matter where it came from.
- A card carries one or more **sources** tried in order, so the same card works on an offline box with a synced library, on an online box, and with plugins like Spotify.
- Cards written by ordinary phone apps (a plain URL) should still do something useful.

## 2. Physical card

- NFC Forum Type 2 tag. **NTAG215** (504 bytes user memory) is the reference; NTAG213 (144 bytes) is too small for most cards, NTAG216 (888 bytes) works.
- ISO card size (85.6 × 54 mm) is the reference shape; stickers and fobs work too.
- Data is a standard **NDEF message** in the Type 2 TLV layout (`0x03` TLV, `0xFE` terminator).

## 3. The tapbox record

- TNF `0x02` (media type), type **`application/vnd.tapbox+json`**.
- Payload: UTF-8 JSON object, compact (no whitespace). Total NDEF message SHOULD fit in 496 bytes (NTAG215).
- A message MAY contain other records. A box MUST use the first record with this type and ignore records it doesn't understand.
- Writers SHOULD put an optional NDEF **URI record first** (e.g. a web page about the content), so a phone that taps the card opens something meaningful. The tapbox record follows.

### Fields

| Key | Type | Required | Meaning |
|---|---|---|---|
| `v` | integer | yes | Spec major version. This draft: `1`. |
| `t` | string | no | Title, ≤ 64 bytes UTF-8. Shown in the web app, may be spoken. |
| `s` | array of strings | one of `s` / `c` | Sources, tried in order. 1–4 entries. |
| `c` | string | one of `s` / `c` | Control command (section 6). |

Rules:

- Exactly one of `s` and `c` MUST be present.
- A box MUST ignore unknown keys (forward compatibility). New keys are added in minor revisions without changing `v`.
- A box MUST reject a card whose `v` is higher than it supports, and SHOULD flash the error state.

## 4. Sources

Each entry in `s` is a URI. A box tries them in order and plays the **first one it can resolve**.

| Scheme | Example | Meaning |
|---|---|---|
| `ni:` ([RFC 6920](https://www.rfc-editor.org/rfc/rfc6920)) | `ni:///sha-256;47DEQpj8…` | Content by SHA-256 hash, base64url without padding. Looked up in the box's library and cache. |
| `https:` | `https://example.org/story.mp3` | An audio file, an M3U/M3U8 playlist, a podcast RSS feed, or a tapbox manifest (section 5). |
| `http:` | `http://nas.local/story.mp3` | As `https:`, for local servers. A box MAY disable it. |
| `spotify:` | `spotify:album:…` | Requires the Spotify plugin. Skipped if not available. |

Resolution rules:

1. `ni:` → if a file with that hash is in the library or cache, play it.
2. `https:`/`http:` → download or stream. If the same card also has an `ni:` source, the box MUST verify the downloaded file against that hash and MUST NOT play it on mismatch. Verified downloads go into the cache so the card works offline next time.
3. Plugin schemes → hand to the plugin if enabled.
4. Unknown schemes are skipped.
5. If nothing resolves, the box flashes the error state and SHOULD record why (for its web app).

A box MUST NOT accept `file:` or any local path from a card. Such a source is skipped like an unknown scheme; a card with nothing else is an error (rule 5).

## 5. Manifests (albums, audiobooks, multi-track)

For content with several tracks, the source points to a **manifest**: a JSON file, referenced by hash and/or URL like any other content.

```json
{
  "tapbox_manifest": 1,
  "title": "Example audiobook",
  "tracks": [
    { "title": "Chapter 1", "hash": "ni:///sha-256;…", "url": "https://example.org/ch1.mp3", "duration": 812 },
    { "title": "Chapter 2", "hash": "ni:///sha-256;…", "url": "https://example.org/ch2.mp3" }
  ]
}
```

- `tapbox_manifest` (integer, required): manifest version, `1`.
- `tracks` (array, required): each track has `hash` and/or `url`; `title` and `duration` (seconds) are optional.
- The manifest's own hash covers every track hash, so one `ni:` on the card pins the whole album.
- Track URLs MUST be verified against track hashes when both are present.

## 6. Control cards

`c` holds a command. A box SHOULD let the owner disable control cards or allow only registered card UIDs.

| Command | Action |
|---|---|
| `bt-pair` | Open Bluetooth pairing for 2 minutes |
| `wifi-setup` | Enter Wi-Fi setup (hotspot) mode |
| `say-name` | Speak the box name and address |
| `vol-max:<0-100>` | Set the maximum volume |
| `sleep:<minutes>` | Sleep timer: stop after N minutes |

## 7. Cards without a tapbox record

For compatibility with cards written by any phone app:

1. **NDEF URI record** with an `https:` audio/playlist/podcast URL → treated as `s: [url]`.
2. **NDEF URI record** with an `https://open.spotify.com/…` link → converted to a `spotify:` URI.
3. **No usable NDEF** (blank card) → look up the card's **UID** in the box's local mapping, set in the web app. UID cards are not portable between boxes.

## 8. Behaviour (recommended)

- Card removed: pause (default) or continue, set per box.
- Same card again: resume from the saved position.
- Different card: start that card from the beginning or its saved position.

## 9. Security

- Card contents are untrusted input. Enforce size limits on JSON (≤ 1 KB), titles and downloads.
- Only the schemes in section 4. No local paths.
- Hash verification is mandatory when a hash is present.
- Writers MAY permanently lock a card (NTAG lock bits). This is irreversible; the web app should warn.

## 10. Test vectors

`card-spec/vectors/` holds encoded examples. Every implementation should decode them to the expected JSON.

**Vector 1** — content card with a hash and a URL (the hash is of an empty file, for illustration):

Payload (125 bytes):

```json
{"v":1,"t":"Example story","s":["ni:///sha-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU","https://example.org/story.mp3"]}
```

Encoded TLV (158 bytes), starting at NTAG page 4:

```
03 9b d2 1b 7d 61 70 70 6c 69 63 61 74 69 6f 6e 2f 76 6e 64 2e 74 61 70 62 6f 78 2b 6a 73 6f 6e
7b 22 76 22 3a 31 2c 22 74 22 3a 22 45 78 61 6d 70 6c 65 20 73 74 6f 72 79 22 2c 22 73 22 3a 5b
22 6e 69 3a 2f 2f 2f 73 68 61 2d 32 35 36 3b 34 37 44 45 51 70 6a 38 48 42 53 61 2d 5f 54 49 6d
57 2d 35 4a 43 65 75 51 65 52 6b 6d 35 4e 4d 70 4a 57 5a 47 33 68 53 75 46 55 22 2c 22 68 74 74
70 73 3a 2f 2f 65 78 61 6d 70 6c 65 2e 6f 72 67 2f 73 74 6f 72 79 2e 6d 70 33 22 5d 7d fe
```

Record header `d2` = MB, ME, SR, TNF 0x02. Payloads over 255 bytes use a long record (SR = 0, 4-byte length) and a 3-byte TLV length (`ff` + 2 bytes).

## 11. Open questions

- Register `application/vnd.tapbox+json` with IANA, or keep `vnd.` unregistered until the name is final?
- Should cards carry an optional cover-image URL for the web app?
- Per-card settings (shuffle, start position) as optional keys?
- A shared public "library index" so boxes can discover URLs for a hash?
