use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name=mastercraft::branding::APP_NAME,
    version,
    about = "SlimeyBeats-EZ-Master native mastering. Use slimeybeats-ez-master.exe for the desktop app."
)]
pub struct Arguments {
    pub inputs: Vec<String>,
    #[arg(short = 'p', long)]
    pub profile: Option<String>,
    #[arg(short='m',long,value_parser=["streaming_safe","loud"])]
    pub mode: Option<String>,
    #[arg(short = 'l', long, allow_hyphen_values = true)]
    pub lufs: Option<f64>,
    #[arg(short = 'c', long, allow_hyphen_values = true)]
    pub ceiling: Option<f64>,
    #[arg(short = 'i', long)]
    pub intensity: Option<f64>,
    #[arg(short = 'f', long, value_delimiter = ',')]
    pub format: Vec<String>,
    #[arg(short = 'o', long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub sr: Option<u32>,
    #[arg(long)]
    pub suffix: Option<String>,
    #[arg(long)]
    pub settings: Option<PathBuf>,
    #[arg(long)]
    pub reference: Option<PathBuf>,
    #[arg(long)]
    pub match_strength: Option<f64>,
    #[arg(long)]
    pub match_loudness: bool,
    #[arg(long)]
    pub no_match_width: bool,
    #[arg(long)]
    pub spotify_strict: bool,
    #[arg(long)]
    pub analyze_only: bool,
    #[arg(long)]
    pub list_profiles: bool,
    #[arg(long, value_delimiter = ',')]
    pub disable: Vec<String>,
    #[arg(long)]
    pub no_safety: bool,
    #[arg(long)]
    pub no_dither: bool,
    #[arg(long)]
    pub no_report: bool,
    #[arg(long)]
    pub report_json: Option<PathBuf>,
    #[arg(long)]
    pub eq_preset: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    pub benchmark: usize,
    #[arg(long)]
    pub calibrate: bool,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long, default_value = "neutral")]
    pub base: String,
    #[arg(long)]
    pub threads: Option<usize>,
}
