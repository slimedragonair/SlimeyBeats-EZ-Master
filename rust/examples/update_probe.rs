//! Explicit live-catalog smoke test. Downloads are inert; installers are never launched.
use clap::Parser;
use mastercraft::app_update::{self, CheckResult};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Parser)]
struct Arguments {
    /// Also download a newer verified installer; never installs it.
    #[arg(long, requires = "cache")]
    download: bool,
    /// Dedicated absolute test cache for the explicit download.
    #[arg(long)]
    cache: Option<PathBuf>,
}
fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = |state| eprintln!("{state:?}");
    let value = match app_update::check(cancel.clone(), &progress)? {
        CheckResult::UpToDate { latest } => {
            serde_json::json!({"status":"up_to_date","current":app_update::current(),"latest":latest})
        }
        CheckResult::Manual {
            release_url,
            reason,
        } => serde_json::json!({"status":"manual","release_url":release_url,"reason":reason}),
        CheckResult::Available(candidate) => {
            let mut value = serde_json::json!({"status":"available","current":app_update::current(),"version":candidate.version(),"build_revision":candidate.build_revision(),"asset":candidate.installer_asset()});
            if arguments.download {
                let cache = arguments
                    .cache
                    .as_deref()
                    .expect("clap requires the cache argument");
                let verified = app_update::download(&candidate, cache, cancel.clone(), &progress)?;
                verified.verify(cancel)?;
                value["download"] = serde_json::json!({"path":verified.path,"sha256":verified.installer_sha256,"bytes":verified.installer_bytes,"verified":true,"launched":false});
            }
            value
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
