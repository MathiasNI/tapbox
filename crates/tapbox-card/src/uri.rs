//! NFC Forum URI record type (`U`) and the phone-written card fallbacks of spec section 7.

/// URI identifier codes, NFC Forum URI RTD 1.0 table 3. Index = code.
const PREFIXES: [&str; 36] = [
    "",
    "http://www.",
    "https://www.",
    "http://",
    "https://",
    "tel:",
    "mailto:",
    "ftp://anonymous:anonymous@",
    "ftp://ftp.",
    "ftps://",
    "sftp://",
    "smb://",
    "nfs://",
    "ftp://",
    "dav://",
    "news:",
    "telnet://",
    "imap:",
    "rtsp://",
    "urn:",
    "pop:",
    "sip:",
    "sips:",
    "tftp:",
    "btspp://",
    "btl2cap://",
    "btgoep://",
    "tcpobex://",
    "irdaobex://",
    "file://",
    "urn:epc:id:",
    "urn:epc:tag:",
    "urn:epc:pat:",
    "urn:epc:raw:",
    "urn:epc:",
    "urn:nfc:",
];

/// Decode a `U` record payload. `None` for an unknown prefix code or invalid UTF-8.
pub(crate) fn decode(payload: &[u8]) -> Option<String> {
    let (&code, rest) = payload.split_first()?;
    let prefix = PREFIXES.get(usize::from(code))?;
    let rest = std::str::from_utf8(rest).ok()?;
    Some(format!("{prefix}{rest}"))
}

/// Encode a URI as a `U` record payload, using the longest matching prefix code.
pub(crate) fn encode(uri: &str) -> Vec<u8> {
    let (code, prefix) = PREFIXES
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, prefix)| uri.starts_with(*prefix))
        .max_by_key(|(_, prefix)| prefix.len())
        .unwrap_or((0, &""));
    let mut payload = Vec::with_capacity(uri.len() + 1);
    payload.push(u8::try_from(code).unwrap_or(0));
    payload.extend_from_slice(&uri.as_bytes()[prefix.len()..]);
    payload
}

/// `https://open.spotify.com/[intl-xx/]<kind>/<id>?…` → `spotify:<kind>:<id>`.
pub(crate) fn spotify_link_to_uri(url: &str) -> Option<String> {
    const KINDS: [&str; 6] = ["track", "album", "playlist", "episode", "show", "artist"];
    let path = url.strip_prefix("https://open.spotify.com/")?;
    let path = path.split(['?', '#']).next()?;
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let mut kind = segments.next()?;
    if kind.starts_with("intl-") {
        kind = segments.next()?;
    }
    let id = segments.next()?;
    let valid_id = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric());
    (KINDS.contains(&kind) && valid_id).then(|| format!("spotify:{kind}:{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_round_trip() {
        for uri in
            ["https://example.org/a.mp3", "https://www.example.org/", "http://nas.local/x", "urn:nfc:x", "weird:thing"]
        {
            assert_eq!(decode(&encode(uri)).as_deref(), Some(uri));
        }
        assert_eq!(encode("https://www.x.org")[0], 0x02);
        assert_eq!(encode("https://x.org")[0], 0x04);
    }

    #[test]
    fn unknown_prefix_code() {
        assert_eq!(decode(&[0x24, b'a']), None);
        assert_eq!(decode(&[]), None);
    }

    #[test]
    fn spotify_links() {
        assert_eq!(
            spotify_link_to_uri("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy?si=abc").as_deref(),
            Some("spotify:album:4aawyAB9vmqN3uQ7FjRGTy")
        );
        assert_eq!(
            spotify_link_to_uri("https://open.spotify.com/intl-no/track/11dFghVXANMlKmJXsNCbNl").as_deref(),
            Some("spotify:track:11dFghVXANMlKmJXsNCbNl")
        );
        assert_eq!(spotify_link_to_uri("https://open.spotify.com/user/x"), None);
        assert_eq!(spotify_link_to_uri("https://example.org/album/x"), None);
    }
}
