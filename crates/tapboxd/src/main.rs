//! `tapboxd` command line. See `tapboxd --help`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tapbox_card::{Card, TagContent, decode_tag, encode_tag};
use tapboxd::bench::{self, Bench, format_hex, parse_hex};
use tapboxd::player::{MpvPlayer, Player, SimPlayer};
use tapboxd::store::Store;

/// `sysexits.h` `EX_CONFIG`. The systemd unit doesn't restart on it
/// (`RestartPreventExitStatus=78`): a broken setup won't fix itself by retrying.
const EX_CONFIG: u8 = 78;

#[derive(Parser)]
#[command(name = "tapboxd", version, about = "The tapbox daemon and its testbench")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive testbench: simulated hardware, real time.
    Sim {
        /// Bench directory (config, cards, library, scenarios).
        #[arg(long, default_value = "testbench")]
        bench: PathBuf,
        /// Simulate audio, or play it through mpv.
        #[arg(long, value_enum, default_value_t = Audio::Sim)]
        audio: Audio,
        /// Start clean: forget saved settings and card links.
        #[arg(long)]
        fresh: bool,
        /// Serve the web app on this address.
        #[arg(long, default_value = "127.0.0.1:8080")]
        web: std::net::SocketAddr,
        /// Don't serve the web app.
        #[arg(long)]
        no_web: bool,
        /// Start as a box with no known Wi-Fi: the web app shows the setup page.
        #[arg(long)]
        setup: bool,
    },
    /// Run scenario scripts on a virtual clock. Exits non-zero on failure.
    Test {
        /// Bench directory.
        #[arg(long, default_value = "testbench")]
        bench: PathBuf,
        /// Scenario files; default: every `.tbx` in `<bench>/scenarios`.
        scenarios: Vec<PathBuf>,
    },
    /// Run on real hardware.
    Run {
        /// Config file.
        #[arg(long, default_value = "/etc/tapbox/tapbox.toml")]
        config: PathBuf,
    },
    /// Encode or decode card bytes.
    #[command(subcommand)]
    Card(CardCmd),
}

#[derive(Subcommand)]
enum CardCmd {
    /// Encode tapbox JSON (e.g. `{"v":1,"s":["https://…"]}`) as tag bytes, in hex.
    Encode {
        /// The card JSON.
        json: String,
        /// URI record to put first, for phones.
        #[arg(long)]
        phone_uri: Option<String>,
    },
    /// Decode tag bytes given in hex.
    Decode {
        /// Tag user memory in hex (spaces allowed).
        hex: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Audio {
    Sim,
    Mpv,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();

    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("error: cannot start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(Cli::parse())) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Cmd::Sim { bench, audio, fresh, web, no_web, setup } => {
            let bench = Bench::load(&bench)?;
            let store = Store::new(bench.config.state_file());
            if fresh {
                for file in [bench.config.state_file(), bench.config.box_file(), bench.config.library_index_file()] {
                    let _ = std::fs::remove_file(file);
                }
            }
            let player = match audio {
                Audio::Sim => Player::Sim(SimPlayer::new(bench::runner::DEFAULT_TRACK_LENGTH)),
                Audio::Mpv => {
                    std::fs::create_dir_all(&bench.config.data_dir)?;
                    Player::Mpv(MpvPlayer::spawn(&bench.config.data_dir.join("mpv.sock")).await?)
                }
            };
            let options = bench::repl::Options { web: (!no_web).then_some(web), wifi_setup: setup };
            bench::repl::run(&bench, player, Some(store), options).await?;
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Test { bench, scenarios } => {
            let bench = Bench::load(&bench)?;
            let scenarios = if scenarios.is_empty() { all_scenarios(&bench.dir.join("scenarios"))? } else { scenarios };
            run_tests(&bench, &scenarios).await
        }
        Cmd::Run { config } => {
            eprintln!(
                "error: hardware drivers are not written yet (config: {}). Use `tapboxd sim` to try the box on this machine.",
                config.display()
            );
            Ok(ExitCode::from(EX_CONFIG))
        }
        Cmd::Card(CardCmd::Encode { json, phone_uri }) => {
            let card = Card::from_json(json.as_bytes()).context("invalid card JSON")?;
            let bytes = encode_tag(&card, phone_uri.as_deref())?;
            println!("{}", format_hex(&bytes));
            eprintln!("{} bytes (NTAG213: 144, NTAG215: 504, NTAG216: 888)", bytes.len());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Card(CardCmd::Decode { hex }) => {
            match decode_tag(&parse_hex(&hex)?)? {
                TagContent::Card(card) => println!("{}", card.to_json()),
                TagContent::Blank => println!("blank: no usable NDEF data (the box would look up the UID)"),
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn all_scenarios(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "tbx"))
        .collect();
    files.sort();
    Ok(files)
}

async fn run_tests(bench: &Bench, scenarios: &[PathBuf]) -> Result<ExitCode> {
    let mut failed = 0;
    for path in scenarios {
        match bench::runner::run_scenario(bench, path).await {
            Ok(()) => println!("ok    {}", path.display()),
            Err(err) => {
                failed += 1;
                println!("FAIL  {err:#}");
            }
        }
    }
    println!("\n{} scenarios, {failed} failed", scenarios.len());
    Ok(if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
