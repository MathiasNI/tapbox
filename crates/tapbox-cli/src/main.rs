//! `tapbox`: make and inspect tapbox cards from the command line.
//!
//! ```text
//! tapbox hash story.mp3
//! tapbox encode --title "Story" --source "$(tapbox hash story.mp3)" --source https://example.org/story.mp3
//! tapbox decode 039bd21b…
//! ```

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tapbox_card::{
    Card, Command, Content, ContentHash, Decoded, Origin, Source, WebUrl, decode_tag, encode_tag,
};

#[derive(Parser)]
#[command(name = "tapbox", version, about = "Make and inspect tapbox NFC cards")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Print the `ni:` content hash of an audio file, for use as a source.
    Hash {
        /// File to hash.
        file: PathBuf,
    },
    /// Encode a card as tag bytes (the data area from NTAG page 4).
    Encode {
        /// Title shown in the web app.
        #[arg(long)]
        title: Option<String>,
        /// A source URI; repeat for fallbacks, tried in order.
        #[arg(long = "source", conflicts_with = "command")]
        sources: Vec<String>,
        /// A control command instead of sources, e.g. `vol-max:60`.
        #[arg(long)]
        command: Option<String>,
        /// Page a phone opens when it taps the card.
        #[arg(long)]
        landing_page: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Hex)]
        format: Format,
    },
    /// Decode tag bytes and print the card as JSON.
    Decode {
        /// Hex bytes; reads stdin when omitted.
        hex: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Lowercase hex on one line.
    Hex,
    /// Raw bytes on stdout.
    Bin,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Action::Hash { file } => hash(&file),
        Action::Encode {
            title,
            sources,
            command,
            landing_page,
            format,
        } => encode(
            title,
            &sources,
            command.as_deref(),
            landing_page.as_deref(),
            format,
        ),
        Action::Decode { hex } => decode(hex),
    }
}

fn hash(path: &PathBuf) -> Result<()> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        hasher.update(chunk);
    }
    println!("{}", ContentHash::from_digest(hasher.finalize().into()));
    Ok(())
}

fn encode(
    title: Option<String>,
    sources: &[String],
    command: Option<&str>,
    landing_page: Option<&str>,
    format: Format,
) -> Result<()> {
    let card = match command {
        Some(command) => {
            if title.is_some() {
                bail!("control cards have no title");
            }
            Card::Control(Command::parse(command)?)
        }
        None => {
            let sources = sources
                .iter()
                .map(|uri| Source::parse(uri).with_context(|| format!("source {uri:?}")))
                .collect::<Result<Vec<_>>>()?;
            Card::Content(Content::new(title, sources)?)
        }
    };
    let landing_page = landing_page.map(WebUrl::parse).transpose()?;
    let bytes = encode_tag(&card, landing_page.as_ref())?;
    if bytes.len() > tapbox_card::NTAG215_DATA_AREA {
        eprintln!(
            "warning: {} bytes is more than an NTAG215 holds ({})",
            bytes.len(),
            tapbox_card::NTAG215_DATA_AREA
        );
    }
    match format {
        Format::Hex => println!("{}", hex::encode(&bytes)),
        Format::Bin => io::stdout().write_all(&bytes)?,
    }
    Ok(())
}

fn decode(hex_arg: Option<String>) -> Result<()> {
    let text = match hex_arg {
        Some(text) => text,
        None => io::read_to_string(io::stdin())?,
    };
    let compact: String = text.split_whitespace().collect();
    let bytes = hex::decode(&compact).context("input is not valid hex")?;
    let output = match decode_tag(&bytes)? {
        Decoded::Card { card, origin } => {
            let card_json: Value = serde_json::from_slice(&card.to_json())?;
            let origin = match origin {
                Origin::Tapbox => "tapbox",
                Origin::UriRecord => "uri-record",
            };
            json!({ "kind": "card", "origin": origin, "card": card_json })
        }
        Decoded::Empty => json!({ "kind": "empty" }),
        Decoded::Unrecognized => json!({ "kind": "unrecognized" }),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
