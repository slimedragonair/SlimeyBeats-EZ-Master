use super::{
    input,
    output::{self, Response, Role},
    paths::ProjectPaths,
    processing,
    request::{Operation, Request},
    settings,
};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    analysis,
    config::Settings,
    studio::{
        stem_archive,
        stem_import_plan::{self, PartSelection, PreferredFormat},
        stem_mix::{self, Stem, StemSet},
    },
    studio_native::{inference::SeparationMode, runtime::StemRuntime, stem_bundle::PreparedImport},
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub const SEPARATION_PCM_BYTES: usize = 512 * 1024 * 1024;

fn load_bank(
    files: &[(mastercraft::studio::stem_mix::StemKind, PathBuf, String)],
    request: &Request,
) -> Result<StemSet> {
    stem_mix::validate_kinds(files.iter().map(|(kind, _, _)| *kind))?;
    let mut bytes = 0usize;
    let mut tracks = Vec::new();
    for (kind, path, _) in files {
        let remaining = request
            .limits
            .max_bank_pcm_bytes
            .checked_sub(bytes)
            .context("Stem bank exceeds configured PCM budget")?;
        let audio = input::load(path, &request.limits, remaining)?;
        bytes = bytes
            .checked_add(audio.samples.len() * 4)
            .context("Stem bank size overflow")?;
        tracks.push(Stem {
            kind: *kind,
            audio: Arc::new(audio),
        });
    }
    let mut bank = StemSet::new(tracks)?;
    for (kind, _, name) in files {
        bank.set_display_name(*kind, name)?;
    }
    Ok(bank)
}

fn explicit_bank(request: &Request, paths: &ProjectPaths) -> Result<StemSet> {
    let files = request
        .stems
        .as_ref()
        .context("Missing stems")?
        .iter()
        .map(|stem| {
            let path = paths.input(&stem.path)?;
            let name = stem
                .name
                .clone()
                .unwrap_or_else(|| stem.kind.name().into_owned());
            Ok((stem.kind, path, name))
        })
        .collect::<Result<Vec<_>>>()?;
    load_bank(&files, request)
}

fn imported_bank(request: &Request, paths: &ProjectPaths, out_dir: &Path) -> Result<StemSet> {
    if request.stems.is_some() {
        return explicit_bank(request, paths);
    }
    let prepared = if let Some(zip) = &request.input {
        PreparedImport::archive(paths.input(zip)?)?
    } else {
        let inputs = request.inputs.as_ref().context("Missing stem inputs")?;
        let mut sources = Vec::new();
        let mut metadata = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let path = paths.input(input.path())?;
            let name = input.name().map(str::to_owned).unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            ensure!(
                !name.is_empty() && name.len() <= 512 && !name.chars().any(char::is_control),
                "Invalid original stem filename"
            );
            // An explicit kind keeps independently selected uploads distinct
            // even when original filenames happen to match.
            let planned_name = input
                .kind()
                .map_or_else(|| name.clone(), |kind| format!("{}/{name}", kind.name()));
            metadata.push(stem_import_plan::ImportFile {
                index,
                format: stem_import_plan::file_format(&name)
                    .context("Stem name needs a supported audio extension")?,
                name: planned_name,
                bytes: path.metadata()?.len(),
                crc32: None,
            });
            sources.push(path);
        }
        PreparedImport {
            source: mastercraft::studio_native::stem_bundle::ImportSource::Files(sources),
            plan: stem_import_plan::plan(metadata)?,
        }
    };
    let selected = prepared
        .plan
        .parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let kinds = part
                .versions
                .iter()
                .filter_map(|version| request.inputs.as_ref()?.get(version.index)?.kind())
                .collect::<std::collections::BTreeSet<_>>();
            ensure!(
                kinds.len() <= 1,
                "Alternate encodings have conflicting stem kinds"
            );
            Ok(PartSelection {
                part_index: index,
                kind: kinds.first().copied(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let selection = stem_import_plan::select(&prepared.plan, &selected, &PreferredFormat::Wav)?;
    // Shared planning/copying, with staging inside this command instead of the
    // desktop adapter's global temp directory. ZIP names never become paths.
    let stage = tempfile::Builder::new()
        .prefix(".stem-import-")
        .tempdir_in(out_dir)?;
    let mut files = Vec::new();
    for (index, (kind, version)) in selection.iter().enumerate() {
        let path = match &prepared.source {
            mastercraft::studio_native::stem_bundle::ImportSource::Files(paths) => paths
                .get(version.index)
                .context("Import source missing")?
                .clone(),
            mastercraft::studio_native::stem_bundle::ImportSource::Archive(zip) => {
                let destination = stage
                    .path()
                    .join(format!("part-{index}.{}", version.format));
                let mut file = BufWriter::new(File::create(&destination)?);
                stem_archive::copy_audio(
                    File::open(zip)?,
                    version,
                    stem_archive::MAX_ENTRY_BYTES,
                    &mut file,
                    &|| false,
                )?;
                file.flush()?;
                destination
            }
        };
        files.push((*kind, path, prepared.plan.parts[index].name.clone()));
    }
    load_bank(&files, request)
}

fn publish_bank(
    bank: &StemSet,
    request: &Request,
    out_dir: &Path,
    response: &mut Response,
    settings: &Settings,
) -> Result<()> {
    let bytes = bank
        .tracks
        .iter()
        .try_fold(0usize, |total, stem| {
            total.checked_add(stem.audio.samples.len() * 4)
        })
        .context("Stem bank size overflow")?;
    ensure!(
        bytes <= request.limits.max_bank_pcm_bytes,
        "Stem bank exceeds configured PCM budget"
    );
    for stem in &bank.tracks {
        let mut asset = output::float(
            &stem.audio,
            out_dir,
            &stem.kind.name(),
            Role::Stem,
            &request.limits,
        )?;
        asset.name = bank.display_name(stem.kind).into_owned();
        asset.kind = Some(stem.kind);
        response.assets.push(asset);
    }
    // Match BrowserStudio::commit_stems: preserve the unnormalized sum for A/B
    // and future revisions, then select the protected full-bank mix as preview.
    let defaults = bank.defaults();
    let mut raw_settings = defaults.clone();
    raw_settings.protect_peaks = false;
    let original = stem_mix::render(bank, &raw_settings, &|| false)?;
    processing::publish(
        &original,
        "original",
        Role::Original,
        out_dir,
        request,
        response,
    )?;
    drop(original);
    let preview = stem_mix::render(bank, &defaults, &|| false)?;
    processing::publish(
        &preview,
        "preview",
        Role::Preview,
        out_dir,
        request,
        response,
    )?;
    response.analysis = Some(analysis::analyze(&preview, &settings::profile(settings)?)?);
    Ok(())
}

pub fn run(
    request: &Request,
    paths: &ProjectPaths,
    out_dir: &Path,
    settings: &Settings,
) -> Result<Response> {
    let mut response = Response::new(request.operation, &request.limits);
    super::progress(0.12, "Loading stems");
    if request.operation == Operation::Separate {
        let path = paths.input(request.input.as_ref().context("Missing input")?)?;
        // Fail before allocating a source beyond the unchanged neural tool cap.
        let audio = input::load(&path, &request.limits, SEPARATION_PCM_BYTES)
            .context("Neural separation retains the 512 MiB decoded PCM tool cap")?;
        let runtime = request.runtime.as_ref().context("Missing runtime")?;
        ensure!(
            runtime.python.is_absolute() && runtime.models.is_absolute(),
            "Runtime paths must be absolute trusted server paths"
        );
        let separated = StemRuntime::configured(runtime.python.clone(), runtime.models.clone())
            .separate(
                &audio,
                request
                    .separation_mode
                    .unwrap_or(SeparationMode::VocalInstrumental),
                out_dir,
                &|p, _| super::progress(0.15 + p * 0.7, "Separating stems"),
                Arc::new(AtomicBool::new(false)),
            )?;
        publish_bank(&separated.stems, request, out_dir, &mut response, settings)?;
        response.tool_metadata = Some(separated.metadata);
    } else {
        let bank = if request.operation == Operation::Mix {
            explicit_bank(request, paths)?
        } else {
            imported_bank(request, paths, out_dir)?
        };
        if request.operation == Operation::Mix {
            super::progress(0.4, "Mixing stems");
            let defaults = bank.defaults();
            let mix = request.mix.as_ref().unwrap_or(&defaults);
            let audio = stem_mix::render(&bank, mix, &|| false)?;
            processing::publish(
                &audio,
                "mix",
                Role::Original,
                out_dir,
                request,
                &mut response,
            )?;
            response.analysis = Some(analysis::analyze(&audio, &settings::profile(settings)?)?);
        } else {
            publish_bank(&bank, request, out_dir, &mut response, settings)?;
        }
    }
    Ok(response)
}
