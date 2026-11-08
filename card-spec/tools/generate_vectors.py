#!/usr/bin/env python3
"""Generate the card-spec test vectors.

This is a deliberately separate, minimal implementation of the encoding in
docs/card-spec.md, so the Rust codec is checked against something other than
itself. Run from the repo root:

    python3 card-spec/tools/generate_vectors.py

Every vector has:
  description   what it covers
  tlv_hex       the tag data area, starting at NTAG page 4
  expected      what a decoder must produce:
                  {"kind": "card", "origin": "tapbox" | "uri-record", "payload": {...}}
                  {"kind": "empty"} | {"kind": "unrecognized"}
                  {"kind": "error"}
  encoder       optional; when present, encoding expected.payload with this
                landing page must give exactly tlv_hex
"""

from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path

MEDIA_TYPE = b"application/vnd.tapbox+json"
OUT_DIR = Path(__file__).resolve().parent.parent / "vectors"


def compact(payload: dict) -> bytes:
    return json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode()


def ni(data: bytes) -> str:
    digest = hashlib.sha256(data).digest()
    return "ni:///sha-256;" + base64.urlsafe_b64encode(digest).rstrip(b"=").decode()


def record(tnf: int, rtype: bytes, payload: bytes, first: bool, last: bool) -> bytes:
    header = tnf
    if first:
        header |= 0x80
    if last:
        header |= 0x40
    if len(payload) < 256:
        header |= 0x10
        length = bytes([len(payload)])
    else:
        length = len(payload).to_bytes(4, "big")
    return bytes([header, len(rtype)]) + length + rtype + payload


def uri_payload(uri: str) -> bytes:
    # Only the prefixes these vectors need; the spec table has 36.
    for code, prefix in ((0x02, "https://www."), (0x04, "https://"), (0x03, "http://")):
        if uri.startswith(prefix):
            return bytes([code]) + uri[len(prefix):].encode()
    return b"\x00" + uri.encode()


def message(records: list[tuple[int, bytes, bytes]]) -> bytes:
    last = len(records) - 1
    return b"".join(
        record(tnf, rtype, payload, i == 0, i == last)
        for i, (tnf, rtype, payload) in enumerate(records)
    )


def tlv(msg: bytes) -> bytes:
    length = bytes([len(msg)]) if len(msg) < 0xFF else b"\xff" + len(msg).to_bytes(2, "big")
    return b"\x03" + length + msg + b"\xfe"


def tapbox_record(payload: dict) -> tuple[int, bytes, bytes]:
    return (0x02, MEDIA_TYPE, compact(payload))


def uri_record(uri: str) -> tuple[int, bytes, bytes]:
    return (0x01, b"U", uri_payload(uri))


def card_vector(description: str, payload: dict, landing_page: str | None = None) -> dict:
    records = ([uri_record(landing_page)] if landing_page else []) + [tapbox_record(payload)]
    return {
        "description": description,
        "tlv_hex": tlv(message(records)).hex(),
        "expected": {"kind": "card", "origin": "tapbox", "payload": payload},
        "encoder": {"landing_page": landing_page},
    }


SPOTIFY_ALBUM = "4aawyAB9vmqN3uQ7FjRGTy"

VECTORS = {
    "01-content-hash-and-url": card_vector(
        "Content card with a SHA-256 source and an HTTPS fallback. "
        "The hash is of an empty file, for illustration only.",
        {"v": 1, "t": "Example story", "s": [ni(b""), "https://example.org/story.mp3"]},
    ),
    "02-control-volume": card_vector(
        "Admin card setting the maximum volume to 60 %.",
        {"v": 1, "c": "vol-max:60"},
    ),
    "03-landing-page-and-spotify": card_vector(
        "URI record first (what a phone opens), then the tapbox record with a Spotify source.",
        {"v": 1, "t": "Bedtime songs", "s": [f"spotify:album:{SPOTIFY_ALBUM}"]},
        landing_page="https://www.example.org/cards/bedtime",
    ),
    "04-long-record": card_vector(
        "Payload over 255 bytes: long NDEF record and three-byte TLV length.",
        {
            "v": 1,
            "t": "Ein langer Titel mit Umlauten: ÆØÅ æøå",
            "s": [
                ni(b"chapter"),
                "https://media.example.org/audiobooks/a-rather-long-path/chapter-01-introduction.mp3",
                "https://mirror.example.net/audiobooks/a-rather-long-path/chapter-01-introduction.mp3",
                f"spotify:show:{SPOTIFY_ALBUM}",
            ],
        },
    ),
    "05-plain-uri-record-spotify-link": {
        "description": "Card written by an ordinary NFC app: only a URI record with an "
        "open.spotify.com link. Decoders convert it to a spotify: source.",
        "tlv_hex": tlv(message([uri_record(f"https://open.spotify.com/album/{SPOTIFY_ALBUM}?si=x")])).hex(),
        "expected": {
            "kind": "card",
            "origin": "uri-record",
            "payload": {"v": 1, "s": [f"spotify:album:{SPOTIFY_ALBUM}"]},
        },
    },
    "06-blank-tag": {
        "description": "Factory-fresh tag: empty NDEF TLV. Decoders fall back to the UID mapping.",
        "tlv_hex": "0300fe" + "00" * 13,
        "expected": {"kind": "empty"},
    },
    "07-unrecognized-vcard": {
        "description": "NDEF with only a vCard record. Nothing tapbox can play.",
        "tlv_hex": tlv(message([(0x02, b"text/vcard", b"BEGIN:VCARD\r\nEND:VCARD")])).hex(),
        "expected": {"kind": "unrecognized"},
    },
    "08-unsupported-version": {
        "description": "Tapbox record from a future major version. Decoders must reject it.",
        "tlv_hex": tlv(message([tapbox_record({"v": 2, "s": ["https://example.org/a.mp3"]})])).hex(),
        "expected": {"kind": "error"},
    },
    "09-file-scheme-rejected": {
        "description": "A card pointing at a local file. Decoders must reject it.",
        "tlv_hex": tlv(message([tapbox_record({"v": 1, "s": ["file:///etc/passwd"]})])).hex(),
        "expected": {"kind": "error"},
    },
}


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for name, vector in VECTORS.items():
        size = len(bytes.fromhex(vector["tlv_hex"]))
        assert size <= 496, f"{name} is {size} bytes, more than an NTAG215 holds"
        path = OUT_DIR / f"{name}.json"
        path.write_text(json.dumps(vector, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"{path.relative_to(OUT_DIR.parent.parent)}  ({size} bytes)")


if __name__ == "__main__":
    main()
