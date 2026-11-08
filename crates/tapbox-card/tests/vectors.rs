//! Every file in `card-spec/vectors/` must decode to its expected payload and
//! re-encode to the exact same bytes.

#![allow(clippy::unwrap_used, reason = "test helper")]

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use tapbox_card::{Card, MEDIA_TYPE, TagContent, decode_tag, encode_tag};

#[derive(Deserialize)]
struct Vector {
    description: String,
    record_type: String,
    payload_utf8: String,
    tlv_hex: String,
}

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../card-spec/vectors")
}

fn from_hex(hex: &str) -> Vec<u8> {
    let digits: Vec<u8> = hex.bytes().filter(u8::is_ascii_hexdigit).collect();
    digits.chunks(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

#[test]
fn all_vectors_decode_and_re_encode() {
    let mut count = 0;
    for entry in fs::read_dir(vectors_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let vector: Vector = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.display();
        assert_eq!(vector.record_type, MEDIA_TYPE, "{name}");

        let expected = Card::from_json(vector.payload_utf8.as_bytes()).unwrap();
        let bytes = from_hex(&vector.tlv_hex);
        assert_eq!(decode_tag(&bytes).unwrap(), TagContent::Card(expected.clone()), "{name}: {}", vector.description);
        assert_eq!(expected.to_json(), vector.payload_utf8, "{name}: canonical JSON");
        assert_eq!(encode_tag(&expected, None).unwrap(), bytes, "{name}: encoding");
        count += 1;
    }
    assert!(count > 0, "no vectors found in {}", vectors_dir().display());
}
