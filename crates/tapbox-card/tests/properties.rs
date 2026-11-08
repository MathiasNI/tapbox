//! Property tests: decoding arbitrary bytes never panics, and encoding then
//! decoding a valid card gives the same card back.

use proptest::prelude::*;
use tapbox_card::{Card, ContentHash, Source, Sources, TagContent, decode_tag, encode_tag};

fn source() -> impl Strategy<Value = Source> {
    prop_oneof![
        any::<[u8; 32]>().prop_map(|digest| Source::Hash(ContentHash::from_digest(digest))),
        "[a-z]{1,12}\\.[a-z]{2,3}/[a-z0-9]{1,20}\\.mp3".prop_map(|rest| Source::Https(format!("https://{rest}"))),
        "[A-Za-z0-9]{22}".prop_map(|id| Source::Spotify(format!("spotify:album:{id}"))),
    ]
}

proptest! {
    #[test]
    fn decoding_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let _ = decode_tag(&bytes);
    }

    #[test]
    fn decoding_mutated_valid_tags_never_panics(
        index in 0usize..200,
        value in any::<u8>(),
        sources in proptest::collection::vec(source(), 1..=4),
    ) {
        let card = Card::content(Some("t"), Sources::new(sources).unwrap()).unwrap();
        let mut bytes = encode_tag(&card, Some("https://example.org/")).unwrap();
        let index = index % bytes.len();
        bytes[index] = value;
        let _ = decode_tag(&bytes);
    }

    #[test]
    fn content_cards_round_trip(
        title in proptest::option::of("[\\PC]{0,20}"),
        sources in proptest::collection::vec(source(), 1..=4),
        phone_uri in proptest::option::of(Just("https://example.org/about".to_owned())),
    ) {
        let title = title.filter(|t| t.len() <= 64);
        let card = Card::content(title.as_deref(), Sources::new(sources).unwrap()).unwrap();
        let bytes = encode_tag(&card, phone_uri.as_deref()).unwrap();
        prop_assert_eq!(decode_tag(&bytes).unwrap(), TagContent::Card(card));
    }
}
