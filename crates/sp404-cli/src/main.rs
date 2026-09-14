//! `sp404`: command-line access to a Roland SP-404MKII over USB.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use sp404_device::{Device, find_ports};
use sp404_dsp::{Tempo, TempoRange};
use sp404_formats::{Padconf, Pattern, Sample, audio, smf, smp, wav};
use sp404_proto::control::{InitScope, MoveMode, PadOp};
use sp404_proto::pad::PadBlock;
use sp404_proto::params::{self, Scope, Target};
use sp404_proto::{PadIndex, ProjectSettings};

#[derive(Parser)]
#[command(
    name = "sp404",
    version,
    about = "Talk to a Roland SP-404MKII over USB"
)]
struct Cli {
    /// Serial port (default: first SP-404MKII found).
    #[arg(long, global = true)]
    port: Option<String>,
    /// Log protocol traffic (-v debug, -vv every message).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Clone, Copy)]
struct BpmRange {
    /// Lowest tempo to report; results are folded into the range.
    #[arg(long, default_value_t = 70.0)]
    min_bpm: f32,
    /// Highest tempo to report.
    #[arg(long, default_value_t = 140.0)]
    max_bpm: f32,
}

impl From<BpmRange> for TempoRange {
    fn from(r: BpmRange) -> Self {
        TempoRange {
            min: r.min_bpm,
            max: r.max_bpm,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// List SP-404MKII serial ports.
    Ports,
    /// Show current project, tempo and free space.
    Status,
    /// List the 16 project slots.
    Projects,
    /// List pads that hold samples.
    Pads,
    /// Show one pad's parameters.
    Pad { pad: PadIndex },
    /// List a directory on the card (default: ROLAND/SP-404MKII).
    Ls { path: Option<String> },
    /// Download a file from the card.
    Get { remote: String, local: PathBuf },
    /// Download a whole project folder (1-based project number).
    ExportProject { project: u8, dir: PathBuf },
    /// Save a pad's sample as WAV.
    SampleToWav {
        pad: PadIndex,
        out: PathBuf,
        /// Project number (default: current).
        #[arg(long)]
        project: Option<u8>,
    },
    /// Print waveform peaks for a pad.
    Peaks {
        pad: PadIndex,
        #[arg(long, default_value_t = 32)]
        points: u32,
        #[arg(long, default_value_t = 256)]
        samples_per_point: u32,
    },
    /// Convert a local pattern file (PTNnnnnn.BIN) to a Standard MIDI File.
    PatternToMidi {
        file: PathBuf,
        out: PathBuf,
        /// Bank tempo in BPM (the official app uses the pattern's bank tempo).
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
    },
    /// Export a pattern from the device as a Standard MIDI File, like the official app.
    ExportPattern {
        /// Pattern slot, A1..J16.
        pattern: PadIndex,
        out: PathBuf,
        /// Project number (default: current).
        #[arg(long)]
        project: Option<u8>,
    },
    /// List pattern slots that hold a pattern.
    Patterns,
    /// Bounce a pattern to a WAV file (the device renders it in real time).
    BouncePattern { pattern: PadIndex, out: PathBuf },
    /// Render each pad a pattern uses to its own WAV file (MULTIPAD export).
    MultipadPattern { pattern: PadIndex, dir: PathBuf },
    /// Play a pad on the device (like holding Preview).
    Preview {
        pad: PadIndex,
        /// How long to hold, in milliseconds.
        #[arg(long, default_value_t = 1000)]
        ms: u64,
    },
    /// Leave the remote screen on the device (the app's MKII EXIT).
    MkiiExit,
    /// Detect tempo and key of a local audio or SMP file.
    Analyze {
        file: PathBuf,
        #[command(flatten)]
        range: BpmRange,
    },
    /// Detect tempo and key of a pad's sample (Start–End), optionally storing the BPM.
    AnalyzePad {
        pad: PadIndex,
        #[command(flatten)]
        range: BpmRange,
        /// Write the detected BPM to the pad (parameter `bpm`).
        #[arg(long)]
        set_bpm: bool,
    },
    /// Show a local PADCONF.BIN (project settings and pads) from a project backup.
    Padconf { file: PathBuf },
    /// Check a local SMP file (header digest, format, size).
    SmpInfo { file: PathBuf },
    /// Convert an audio file (WAV, AIFF, FLAC, MP3) to SMP locally.
    #[command(alias = "wav-to-smp")]
    ToSmp { input: PathBuf, out: PathBuf },
    /// List known parameter names.
    Params,
    /// Debug: save raw live state (project settings and all 160 pad blocks) to a folder.
    #[command(hide = true)]
    DumpState { dir: PathBuf },
    /// Debug: send a raw message (hex payload) and print everything received.
    Raw {
        /// Channel number (5 control, 6 file).
        #[arg(long, default_value_t = 5)]
        channel: u8,
        /// Send as a long message instead of a short one.
        #[arg(long)]
        long: bool,
        /// Milliseconds to listen after sending.
        #[arg(long, default_value_t = 1500)]
        wait: u64,
        /// Payload bytes in hex, e.g. "fe 66 00" (short: up to 7 bytes).
        hex: Vec<String>,
    },

    // ---- commands below change the device ----
    /// Set a parameter: `set B1 level 100`, `set global tempo-select 1`, `set global bank-tempo-j 12000`.
    Set {
        target: String,
        param: String,
        value: i32,
    },
    /// Import an audio file (WAV, AIFF, FLAC, MP3) onto a pad.
    #[command(alias = "import-wav")]
    Import {
        pad: PadIndex,
        file: PathBuf,
        #[arg(long)]
        project: Option<u8>,
        /// Sample name (default: file stem).
        #[arg(long)]
        name: Option<String>,
    },
    /// Delete a pad's sample.
    DeleteSample { pad: PadIndex },
    /// Truncate a pad's sample to its start/end points.
    Truncate { pad: PadIndex },
    /// Normalize a pad's sample.
    Normalize { pad: PadIndex },
    /// Rename a pad's sample.
    RenameSample { pad: PadIndex, name: String },
    /// Rename a project (1-based project number).
    RenameProject { project: u8, name: String },
    /// Move a pad's sample to another pad (replacing it), or swap them with --exchange.
    MoveSample {
        from: PadIndex,
        to: PadIndex,
        #[arg(long)]
        exchange: bool,
    },
    /// Make a project (1-based) the current project.
    SelectProject { project: u8 },
    /// Erase a project's data (like the app's Init). Selects the project first.
    InitProject {
        project: u8,
        /// all, samples, samples-bank, patterns, patterns-bank
        #[arg(long, default_value = "all")]
        what: String,
        /// Bank letter for samples-bank / patterns-bank.
        #[arg(long)]
        bank: Option<char>,
        /// Required: this erases data on the card.
        #[arg(long)]
        yes: bool,
    },
    /// Overwrite a project (1-based) with a backup folder (the one holding PADCONF.BIN),
    /// like the app's "Import to MKII".
    RestoreProject {
        project: u8,
        dir: PathBuf,
        /// Required: the project's current contents are overwritten.
        #[arg(long)]
        yes: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    env_logger::Builder::new().filter_level(level).init();

    match &cli.command {
        Command::Ports => return ports(),
        Command::SmpInfo { file } => return smp_info(file),
        Command::Padconf { file } => return padconf(file),
        Command::Analyze { file, range } => return analyze_file(file, (*range).into()),
        Command::PatternToMidi { file, out, bpm } => return pattern_to_midi(file, out, *bpm),
        Command::ToSmp { input, out } => return to_smp(input, out),
        Command::Params => return list_params(),
        _ => {}
    }

    let dev = match &cli.port {
        Some(p) => Device::open(p),
        None => Device::open_first(),
    }
    .context("opening SP-404MKII (is the official app still connected?)")?;

    match cli.command {
        Command::Status => status(&dev),
        Command::Projects => projects(&dev),
        Command::Pads => pads(&dev),
        Command::Pad { pad } => show_pad(&dev, pad),
        Command::Ls { path } => ls(&dev, path.as_deref().unwrap_or("ROLAND/SP-404MKII")),
        Command::Get { remote, local } => {
            let data = dev.read_file(&remote)?;
            fs::write(&local, &data)?;
            println!("{} bytes -> {}", data.len(), local.display());
            Ok(())
        }
        Command::ExportProject { project, dir } => export_project(&dev, project, &dir),
        Command::SampleToWav { pad, out, project } => sample_to_wav(&dev, pad, &out, project),
        Command::Peaks {
            pad,
            points,
            samples_per_point,
        } => {
            for ch in 0..2 {
                let peaks = dev.peaks(pad, ch, 0, points, samples_per_point)?;
                println!("ch{ch}: {peaks:?}");
            }
            Ok(())
        }
        Command::Set {
            target,
            param,
            value,
        } => set(&dev, &target, &param, value),
        Command::Import {
            pad,
            file,
            project,
            name,
        } => import(&dev, pad, &file, project, name),
        Command::DeleteSample { pad } => Ok(dev.pad_op(PadOp::Delete, pad)?),
        Command::Truncate { pad } => Ok(dev.pad_op(PadOp::Truncate, pad)?),
        Command::Normalize { pad } => Ok(dev.pad_op(PadOp::Normalize, pad)?),
        Command::RenameSample { pad, name } => Ok(dev.set_sample_name(pad, &name)?),
        Command::RenameProject { project, name } => {
            Ok(dev.set_project_name(project_index(project)?, &name)?)
        }
        Command::DumpState { dir } => dump_state(&dev, &dir),
        Command::AnalyzePad {
            pad,
            range,
            set_bpm,
        } => analyze_pad(&dev, pad, range.into(), set_bpm),
        Command::ExportPattern {
            pattern,
            out,
            project,
        } => export_pattern(&dev, pattern, &out, project),
        Command::Patterns => {
            for slot in PadIndex::all() {
                if dev.pattern_exists(slot)? {
                    println!("{slot}");
                }
            }
            Ok(())
        }
        Command::BouncePattern { pattern, out } => {
            let mut file = fs::File::create(&out)?;
            dev.bounce_pattern(pattern, &mut file)?;
            println!(
                "{pattern}: {} bytes -> {}",
                file.metadata()?.len(),
                out.display()
            );
            Ok(())
        }
        Command::MultipadPattern { pattern, dir } => multipad(&dev, pattern, &dir),
        Command::Preview { pad, ms } => Ok(dev.preview(pad, std::time::Duration::from_millis(ms))?),
        Command::MkiiExit => Ok(dev.mkii_exit()?),
        Command::MoveSample { from, to, exchange } => {
            let mode = if exchange {
                MoveMode::Exchange
            } else {
                MoveMode::Overwrite
            };
            Ok(dev.move_sample(from, to, mode)?)
        }
        Command::RestoreProject { project, dir, yes } => restore_project(&dev, project, &dir, yes),
        Command::InitProject {
            project,
            what,
            bank,
            yes,
        } => init_project(&dev, project, &what, bank, yes),
        Command::SelectProject { project } => Ok(dev.select_project(project_index(project)?)?),
        Command::Raw {
            channel,
            long,
            wait,
            hex,
        } => raw(&dev, channel, long, wait, &hex),
        Command::Ports
        | Command::SmpInfo { .. }
        | Command::Padconf { .. }
        | Command::PatternToMidi { .. }
        | Command::Analyze { .. }
        | Command::ToSmp { .. }
        | Command::Params => {
            unreachable!()
        }
    }
}

fn ports() -> Result<()> {
    let ports = find_ports()?;
    if ports.is_empty() {
        println!("no SP-404MKII found");
    }
    for p in ports {
        println!(
            "{}\t{}\t{}",
            p.name,
            p.product.unwrap_or_default(),
            p.serial_number.unwrap_or_default()
        );
    }
    Ok(())
}

fn project_index(project: u8) -> Result<u8> {
    if !(1..=16).contains(&project) {
        bail!("project must be 1..16");
    }
    Ok(project - 1)
}

fn current_project(dev: &Device, project: Option<u8>) -> Result<u8> {
    match project {
        Some(p) => project_index(p).map(|i| i + 1),
        None => Ok(dev.status()?.project + 1),
    }
}

fn status(dev: &Device) -> Result<()> {
    let st = dev.status()?;
    println!("port:          {}", dev.port_name());
    println!("project:       {} ({})", st.project + 1, st.settings.name());
    print_settings(&st.settings);
    println!(
        "free space:    {:.1} GB",
        f64::from(dev.free_kb()?) / 1024.0 / 1024.0
    );
    log::debug!("status payload: {:02x?}", st.raw);
    Ok(())
}

fn projects(dev: &Device) -> Result<()> {
    for (i, name) in dev.project_names()?.iter().enumerate() {
        println!("{:2}  {}", i + 1, if name.is_empty() { "-" } else { name });
    }
    Ok(())
}

fn bpm(hundredths: u16) -> String {
    format!("{:.2}", f64::from(hundredths) / 100.0)
}

fn print_settings(settings: &ProjectSettings) {
    println!(
        "project tempo: {}{}",
        bpm(settings.project_tempo()),
        if settings.uses_project_tempo() {
            "  (tempo select: project)"
        } else {
            "  (tempo select: bank)"
        }
    );
    for bank in 0..10 {
        let b = settings.bank(bank);
        println!(
            "bank {}:        tempo {:>6}  volume {:3}{}",
            (b'A' + bank as u8) as char,
            bpm(b.tempo),
            b.volume,
            if b.protected { "  protected" } else { "" }
        );
    }
}

fn pads(dev: &Device) -> Result<()> {
    let project = dev.current_project()? + 1;
    let blocks = dev.pad_blocks()?;
    // The channel count is only in the SMP header, as the official app also finds.
    let mut channels = std::collections::HashMap::new();
    for block in blocks.iter().filter(|b| b.has_sample()) {
        let path = block.pad.sample_path(project);
        if let Some(head) = dev.read_prefix(&path, smp::HEADER_LEN as u32)? {
            if let Ok(info) = smp::HeaderInfo::parse(&head) {
                channels.insert(block.pad, info.channels);
            }
        }
    }
    print_pads(&blocks, |pad| channels.get(&pad).copied());
    Ok(())
}

/// `channels` gives a pad's channel count when known; stereo is assumed otherwise.
fn print_pads(blocks: &[PadBlock], channels: impl Fn(PadIndex) -> Option<u16>) {
    for block in blocks {
        if !block.has_sample() {
            continue;
        }
        let known = channels(block.pad);
        let ch = u32::from(known.unwrap_or(2));
        let frames = (block.file_size() - smp::HEADER_LEN as u32) / (2 * ch);
        println!(
            "{:4} {:>9} frames {:6}  level {:3}  bpm {:6.2}  {}",
            block.pad.to_string(),
            frames,
            match known {
                Some(1) => "mono",
                Some(_) => "stereo",
                None => "",
            },
            block.param(0x69).unwrap_or(0),
            f64::from(block.param(0x6F).unwrap_or(0)) / 100.0,
            block.name()
        );
    }
}

fn show_pad(dev: &Device, pad: PadIndex) -> Result<()> {
    let block = dev.pad_block(pad)?;
    println!(
        "pad {pad}: {}",
        if block.has_sample() {
            block.name()
        } else {
            "(empty)".into()
        }
    );
    if block.has_sample() {
        let (start, end) = block.start_end_bytes();
        println!(
            "  file size  {} bytes, start {start}, end {end} (byte offsets)",
            block.file_size()
        );
    }
    for p in params::PARAMS.iter().filter(|p| p.scope == Scope::Pad) {
        if let Some(v) = block.param(p.id) {
            println!("  {:14} {v}", p.name);
        }
    }
    let chops: Vec<String> = block
        .chop_points()
        .iter()
        .flatten()
        .map(u32::to_string)
        .collect();
    if !chops.is_empty() {
        println!("  chop points {}", chops.join(", "));
    }
    Ok(())
}

fn ls(dev: &Device, path: &str) -> Result<()> {
    for e in dev.list_dir(path)? {
        let child = format!("{}/{}", path.trim_end_matches('/'), e.name);
        if e.is_dir() {
            println!("{:>10}  {}/", "", e.name);
        } else {
            let size = dev.stat(&child)?.map(|s| s.size).unwrap_or(0);
            println!("{size:>10}  {}", e.name);
        }
    }
    Ok(())
}

fn export_project(dev: &Device, project: u8, dir: &Path) -> Result<()> {
    project_index(project)?;
    let root = format!("ROLAND/SP-404MKII/PROJECT_{project:02}");
    let mut stack = vec![root.clone()];
    let mut files = 0usize;
    let mut bytes = 0usize;
    while let Some(remote_dir) = stack.pop() {
        let local_dir = dir.join(&remote_dir);
        fs::create_dir_all(&local_dir)?;
        for e in dev.list_dir(&remote_dir)? {
            let child = format!("{remote_dir}/{}", e.name);
            if e.is_dir() {
                stack.push(child);
            } else {
                let data = dev
                    .read_file(&child)
                    .with_context(|| format!("reading {child}"))?;
                fs::write(local_dir.join(&e.name), &data)?;
                println!("{:>10}  {child}", data.len());
                files += 1;
                bytes += data.len();
            }
        }
    }
    println!(
        "{files} files, {bytes} bytes -> {}",
        dir.join(root).display()
    );
    Ok(())
}

fn sample_to_wav(dev: &Device, pad: PadIndex, out: &Path, project: Option<u8>) -> Result<()> {
    let project = current_project(dev, project)?;
    let path = pad.sample_path(project);
    let bytes = dev
        .read_file(&path)
        .with_context(|| format!("reading {path}"))?;
    let sample = Sample::from_smp(&bytes)?;
    wav::write(&sample, out)?;
    println!("{pad}: {} frames -> {}", sample.frames(), out.display());
    Ok(())
}

fn pattern_to_midi(file: &Path, out: &Path, bpm: f64) -> Result<()> {
    let pattern = Pattern::parse(&fs::read(file)?)?;
    let tempo = (bpm * 100.0).round().clamp(1.0, f64::from(u16::MAX)) as u16;
    fs::write(out, smf::from_pattern(&pattern, tempo))?;
    println!(
        "{} events, {} ticks -> {}",
        pattern.events.len(),
        pattern.length,
        out.display()
    );
    Ok(())
}

fn export_pattern(dev: &Device, slot: PadIndex, out: &Path, project: Option<u8>) -> Result<()> {
    let project = current_project(dev, project)?;
    let path = format!(
        "ROLAND/SP-404MKII/PROJECT_{project:02}/PTN/PTN{:05}.BIN",
        slot.index() + 1
    );
    let bytes = dev
        .read_file(&path)
        .with_context(|| format!("reading {path} (is there a pattern in {slot}?)"))?;
    let pattern = Pattern::parse(&bytes)?;
    let settings = dev.project_settings(project - 1)?;
    let tempo = settings.bank(usize::from(slot.bank())).tempo;
    fs::write(out, smf::from_pattern(&pattern, tempo))?;
    println!(
        "{slot}: {} events at {} BPM -> {}",
        pattern.events.len(),
        bpm(tempo),
        out.display()
    );
    Ok(())
}

fn init_project(
    dev: &Device,
    project: u8,
    what: &str,
    bank: Option<char>,
    yes: bool,
) -> Result<()> {
    let index = project_index(project)?;
    let bank_index = || -> Result<u8> {
        let b = bank.context("--bank A..J is required for a bank init")?;
        let b = b.to_ascii_uppercase();
        if !('A'..='J').contains(&b) {
            bail!("bank must be A..J");
        }
        Ok(b as u8 - b'A')
    };
    let scope = match what {
        "all" => InitScope::All,
        "samples" => InitScope::AllSamples,
        "samples-bank" => InitScope::SamplesBank(bank_index()?),
        "patterns" => InitScope::AllPatterns,
        "patterns-bank" => InitScope::PatternsBank(bank_index()?),
        other => bail!("unknown --what {other:?}"),
    };
    let name = dev.project_names()?[usize::from(index)].clone();
    if !yes {
        bail!("this erases {scope:?} of project {project} ({name}); run again with --yes");
    }
    if dev.current_project()? != index {
        dev.select_project(index)?;
        println!("selected project {project}");
    }
    dev.init_project(index, scope)?;
    println!("project {project}: {scope:?} erased");
    Ok(())
}

fn restore_project(dev: &Device, project: u8, dir: &Path, yes: bool) -> Result<()> {
    let index = project_index(project)?;
    if !dir.join("PADCONF.BIN").is_file() {
        bail!("{} has no PADCONF.BIN", dir.display());
    }
    // The app writes PADCONF first, then pictures, the pattern chain, patterns, samples.
    let mut files = vec!["PADCONF.BIN".to_string()];
    for sub in ["PICTURE", "PTN", "SMPL"] {
        let mut names: Vec<String> = match fs::read_dir(dir.join(sub)) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort_by_key(|n| (!n.ends_with(".CHN"), n.clone()));
        files.extend(names.into_iter().map(|n| format!("{sub}/{n}")));
    }
    let data = files
        .into_iter()
        .map(|f| {
            let bytes = fs::read(dir.join(&f)).with_context(|| format!("reading {f}"))?;
            Ok((f, bytes))
        })
        .collect::<Result<Vec<_>>>()?;
    let total: usize = data.iter().map(|(_, d)| d.len()).sum();
    let name = Padconf::parse(&data[0].1)?.settings().name();
    println!(
        "{} files, {total} bytes, project name {name:?} -> project {project}",
        data.len()
    );
    if !yes {
        bail!("this overwrites project {project}; run again with --yes");
    }
    let free = u64::from(dev.free_kb()?) * 1024;
    if total as u64 > free {
        bail!("not enough space on the card ({free} bytes free)");
    }
    // Restoring erases the current project first, so the target must be selected.
    if dev.current_project()? != index {
        dev.select_project(index)?;
        println!("selected project {project}");
    }
    dev.restore_project(index, &data, |i, f| {
        println!("{:>3}/{}  {f}", i + 1, data.len())
    })?;
    println!("project {project} restored");
    Ok(())
}

fn multipad(dev: &Device, pattern: PadIndex, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let pads = dev.pattern_pads(pattern)?;
    for (i, pad) in pads.iter().enumerate() {
        let name = dev.pad_block(*pad)?.name();
        let name = if name.is_empty() {
            "EMPTY".into()
        } else {
            name
        };
        let file_name: String = format!(
            "{}-{}-{}.WAV",
            (b'A' + pad.bank() as u8) as char,
            pad.pad() + 1,
            name.trim_end()
        )
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
        let path = dir.join(file_name);
        let mut file = fs::File::create(&path)?;
        let rendered = dev.render_pattern_pad(pattern, *pad, &mut file);
        if let Err(e) = rendered {
            drop(file);
            fs::remove_file(&path).ok();
            return Err(e).with_context(|| format!("rendering {pad}"));
        }
        println!("{}/{}: {pad} -> {}", i + 1, pads.len(), path.display());
    }
    Ok(())
}

/// Print tempo and key findings for mono audio at 48 kHz.
fn report_analysis(mono: &[f32], range: TempoRange) -> Option<Tempo> {
    let seconds = mono.len() as f64 / f64::from(smp::SAMPLE_RATE);
    println!("length        {seconds:.3} s");
    let tempo = sp404_dsp::tempo::detect(mono, smp::SAMPLE_RATE, range);
    match tempo {
        Some(t) => println!(
            "tempo         {:.2} BPM (confidence {:.2})",
            t.bpm, t.confidence
        ),
        None => println!("tempo         not detected"),
    }
    if let Some(bpm) = sp404_dsp::tempo::from_length(seconds, range) {
        println!("by length     {bpm:.2} BPM (whole bars)");
    }
    match sp404_dsp::key::detect(mono, smp::SAMPLE_RATE) {
        Some(k) => println!(
            "key           {} (confidence {:.2}), name suffix {:?}",
            k.name(),
            k.confidence,
            k.suffix()
        ),
        None => println!("key           not detected"),
    }
    tempo
}

fn analyze_file(file: &Path, range: TempoRange) -> Result<()> {
    let bytes = fs::read(file)?;
    let sample = if bytes.starts_with(b"RFWV") {
        Sample::from_smp(&bytes)?
    } else {
        load_audio(file)?
    };
    let mono = sp404_dsp::mono_from_i16(&sample.samples, usize::from(sample.channels));
    report_analysis(&mono, range);
    Ok(())
}

fn analyze_pad(dev: &Device, pad: PadIndex, range: TempoRange, set_bpm: bool) -> Result<()> {
    let block = dev.pad_block(pad)?;
    if !block.has_sample() {
        bail!("{pad} is empty");
    }
    let project = dev.current_project()? + 1;
    let bytes = dev.read_file(&pad.sample_path(project))?;
    let sample = Sample::from_smp(&bytes)?;
    let channels = usize::from(sample.channels);
    // Analyse the Start–End region, as that is what the pad plays.
    let (start, end) = block.start_end_bytes();
    let to_index = |offset: u32| {
        (offset.saturating_sub(smp::HEADER_LEN as u32) as usize / 2).min(sample.samples.len())
    };
    let (from, to) = (to_index(start), to_index(end));
    let region = &sample.samples[from - from % channels..to - to % channels];
    println!("{pad}: {}", block.name());
    let mono = sp404_dsp::mono_from_i16(region, channels);
    let tempo = report_analysis(&mono, range);
    if set_bpm {
        let tempo = tempo.context("no tempo to store")?;
        let value = (tempo.bpm * 100.0).round() as i32;
        dev.set_param(0x6F, Target::Pad(pad), value.clamp(4000, 20000))?;
        println!("stored bpm {:.2} on {pad}", f64::from(value) / 100.0);
    }
    Ok(())
}

fn padconf(file: &Path) -> Result<()> {
    let pc = Padconf::parse(&fs::read(file)?)?;
    println!("project name:  {}", pc.settings().name());
    print_settings(pc.settings());
    println!();
    print_pads(pc.pads(), |_| None);
    Ok(())
}

fn smp_info(file: &Path) -> Result<()> {
    let bytes = fs::read(file)?;
    println!("digest ok: {}", smp::digest_matches(&bytes));
    let sample = Sample::from_smp(&bytes)?;
    println!(
        "{} channels, {} frames, {:.3} s",
        sample.channels,
        sample.frames(),
        sample.frames() as f64 / 48_000.0
    );
    Ok(())
}

/// Decode an audio file for import, reporting any conversion.
fn load_audio(input: &Path) -> Result<Sample> {
    let imported = audio::read(input).with_context(|| format!("reading {}", input.display()))?;
    if imported.resampled() {
        eprintln!(
            "note: resampled {} Hz -> 48000 Hz (48 kHz sources import without conversion)",
            imported.source_rate
        );
    }
    if imported.source_channels > 2 {
        eprintln!(
            "note: {}-channel source, kept the first two channels",
            imported.source_channels
        );
    }
    Ok(imported.sample)
}

fn to_smp(input: &Path, out: &Path) -> Result<()> {
    let sample = load_audio(input)?;
    fs::write(out, sample.to_smp())?;
    println!("{} frames -> {}", sample.frames(), out.display());
    Ok(())
}

fn list_params() -> Result<()> {
    for p in params::PARAMS {
        let scope = if p.scope == Scope::Pad {
            "pad"
        } else {
            "global"
        };
        println!(
            "{:14} 0x{:02x} {:6} {:>6}..{:<6} {}",
            p.name, p.id, scope, p.min, p.max, p.help
        );
    }
    println!(
        "bank-protect-<a..j>, bank-tempo-<a..j> (BPM x 100), bank-volume-<a..j> (0..127)  global"
    );
    Ok(())
}

fn set(dev: &Device, target: &str, param: &str, value: i32) -> Result<()> {
    let is_global = target.eq_ignore_ascii_case("global");
    if let Some((id, inverted)) = params::bank_param(param) {
        if !is_global {
            bail!("{param} is a global parameter; use `set global {param} <value>`");
        }
        let v = if inverted {
            127 - value.clamp(0, 127)
        } else {
            value
        };
        return Ok(dev.set_param(id, Target::Global, v)?);
    }
    let info = params::by_name(param)
        .with_context(|| format!("unknown parameter {param:?} (see `sp404 params`)"))?;
    if !(info.min..=info.max).contains(&value) {
        bail!("{} must be in {}..{}", info.name, info.min, info.max);
    }
    let target = match (info.scope, is_global) {
        (Scope::Global, true) => Target::Global,
        (Scope::Pad, false) => Target::Pad(target.parse()?),
        (Scope::Global, false) => bail!("{param} is a global parameter"),
        (Scope::Pad, true) => bail!("{param} is a pad parameter"),
    };
    Ok(dev.set_param(info.id, target, value)?)
}

fn dump_state(dev: &Device, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("status.bin"), dev.status()?.raw)?;
    fs::write(dir.join("settings.bin"), dev.status()?.settings.raw())?;
    for block in dev.pad_blocks()? {
        fs::write(dir.join(format!("pad-{}.bin", block.pad)), block.raw())?;
    }
    println!("saved to {}", dir.display());
    Ok(())
}

fn raw(dev: &Device, channel: u8, long: bool, wait: u64, hex: &[String]) -> Result<()> {
    let bytes: Vec<u8> = hex
        .join("")
        .split_whitespace()
        .collect::<String>()
        .as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap_or("zz"), 16))
        .collect::<Result<_, _>>()
        .context("payload must be hex bytes")?;
    let channel = sp404_proto::Channel::from_byte(channel);
    let msg = if long {
        sp404_proto::Message::long(channel, bytes)
    } else {
        if bytes.len() > sp404_proto::frame::SHORT_MAX {
            bail!(
                "short messages carry at most {} bytes; use --long",
                sp404_proto::frame::SHORT_MAX
            );
        }
        sp404_proto::Message::short(channel, &bytes)
    };
    for m in dev.raw_exchange(&msg, std::time::Duration::from_millis(wait))? {
        let kind = if m.is_short() { "short" } else { "long " };
        let p: Vec<String> = m.payload().iter().map(|b| format!("{b:02x}")).collect();
        println!(
            "< ch{:02x} {kind} dst={:02x} {}",
            m.channel.to_byte(),
            m.destination,
            p.join(" ")
        );
    }
    Ok(())
}

fn import(
    dev: &Device,
    pad: PadIndex,
    input: &Path,
    project: Option<u8>,
    name: Option<String>,
) -> Result<()> {
    let sample = load_audio(input)?;
    let name = name.unwrap_or_else(|| {
        input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let project = current_project(dev, project)?;
    let smp_bytes = sample.to_smp();
    dev.import_smp(project, pad, &smp_bytes, &name)?;
    println!(
        "{pad}: imported {} frames as {name:?} into project {project}",
        sample.frames()
    );
    Ok(())
}
