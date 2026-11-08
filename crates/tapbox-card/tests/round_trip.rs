//! Property tests: any valid card survives encode → decode unchanged, and the
//! decoder never panics on arbitrary bytes.

// Strategies run outside `#[test]` functions, where clippy's test exemption
// doesn't reach; the generated values are valid by construction.
#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use tapbox_card::{
    Card, Command, Content, ContentHash, Decoded, Origin, Source, WebUrl, decode_tag, encode_tag,
};

fn source() -> impl Strategy<Value = Source> {
    prop_oneof![
        any::<[u8; 32]>().prop_map(|d| Source::Hash(ContentHash::from_digest(d))),
        "[a-z]{1,20}\\.[a-z]{2,5}/[a-zA-Z0-9/_.-]{0,60}"
            .prop_map(|rest| Source::parse(&format!("https://{rest}")).unwrap()),
        "[a-zA-Z0-9]{22}".prop_map(|id| Source::parse(&format!("spotify:track:{id}")).unwrap()),
    ]
}

fn card() -> impl Strategy<Value = Card> {
    let content = (
        proptest::option::of("[^\\u{0}-\\u{1f}]{1,20}"),
        proptest::collection::vec(source(), 1..=4),
    )
        .prop_map(|(title, sources)| Card::Content(Content::new(title, sources).unwrap()));
    let control = prop_oneof![
        Just(Command::BluetoothPair),
        Just(Command::WifiSetup),
        Just(Command::SayName),
        (0u8..=100).prop_map(Command::VolumeMax),
        (1u16..=720).prop_map(Command::Sleep),
    ]
    .prop_map(Card::Control);
    prop_oneof![content, control]
}

proptest! {
    #[test]
    fn valid_cards_round_trip(card in card(), with_landing in any::<bool>()) {
        let landing = with_landing.then(|| WebUrl::parse("https://example.org/c").unwrap());
        let bytes = encode_tag(&card, landing.as_ref()).unwrap();
        prop_assert_eq!(decode_tag(&bytes), Ok(Decoded::Card { card, origin: Origin::Tapbox }));
    }

    #[test]
    fn decoder_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let _ = decode_tag(&bytes);
    }

    #[test]
    fn decoder_never_panics_inside_a_valid_tlv(message in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut area = vec![0x03, 0xFF];
        area.extend_from_slice(&u16::try_from(message.len()).unwrap().to_be_bytes());
        area.extend_from_slice(&message);
        area.push(0xFE);
        let _ = decode_tag(&area);
    }
}
