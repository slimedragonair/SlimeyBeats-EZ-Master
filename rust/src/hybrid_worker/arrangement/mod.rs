//! Persistent normal-worker actor for server-side arrangement editing/playback.
mod archive;
mod fork;
mod protocol;
mod workspace;

use super::paths::ProjectPaths;
use anyhow::{Result, ensure};
use protocol::Envelope;
use std::{
    io::{BufRead, Read, Write},
    path::Path,
};

const MAX_LINE_BYTES: usize = 17 * 1024 * 1024;

pub fn run(root: &Path, threads: usize) -> Result<()> {
    let paths = ProjectPaths::new(root)?;
    // Configure only this owned child, before codecs or Rayon start.
    unsafe {
        for key in ["TMP", "TEMP", "TMPDIR"] {
            std::env::set_var(key, &paths.root);
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()?;
    let mut workspace = workspace::Workspace::new(paths);
    let input = std::io::stdin();
    let mut input = input.lock();
    let output = std::io::stdout();
    let mut output = output.lock();
    loop {
        let mut line = Vec::new();
        let size = (&mut input)
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if size == 0 {
            break;
        }
        ensure!(
            size <= MAX_LINE_BYTES && line.last() == Some(&b'\n'),
            "Arrangement request exceeds its framing budget"
        );
        let json = serde_json::from_slice::<serde_json::Value>(&line);
        let id = json
            .as_ref()
            .ok()
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_u64());
        let envelope = json.and_then(serde_json::from_value::<Envelope>);
        let response = match envelope {
            Ok(envelope) => match workspace.execute(envelope.request) {
                Ok(value) => serde_json::json!({"id":envelope.id,"value":value}),
                // Private pipe only. HTTP never forwards native error strings or paths.
                Err(error) => serde_json::json!({"id":envelope.id,"error":error.to_string()}),
            },
            Err(_) => serde_json::json!({"id":id,"error":"Invalid arrangement request"}),
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
