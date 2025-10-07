use crate::config::Config;
use crate::monitoring::MonitoringHub;
use crate::recording::RecordingState;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use rubato::{Resampler, SincFixedIn};
use sameold::{Message as SameMessage, SameReceiverBuilder};
use std::future::pending;
use std::io::{Read, Result as IoResult};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender as TokioSender;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{error, info, warn};

const TARGET_SAMPLE_RATE: u32 = 48000;

fn stream_inactivity_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(120)
}

struct ChannelReader {
    rx: crossbeam_channel::Receiver<Bytes>,
    buffer: Bytes,
    pos: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.pos >= self.buffer.len() {
            match self.rx.recv() {
                Ok(new_buffer) => {
                    self.buffer = new_buffer;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let bytes_to_copy = (self.buffer.len() - self.pos).min(buf.len());
        let end = self.pos + bytes_to_copy;
        buf[..bytes_to_copy].copy_from_slice(&self.buffer[self.pos..end]);
        self.pos = end;
        Ok(bytes_to_copy)
    }
}

pub async fn run_audio_processor(
    config: Config,
    tx: TokioSender<(String, String, String, String, Duration, String)>,
    recording_state: Arc<Mutex<Option<RecordingState>>>,
    nnnn_tx: BroadcastSender<()>,
    monitoring: MonitoringHub,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .http1_only()
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .pool_idle_timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("build reqwest client")?;

    for stream_url in config.icecast_stream_urls {
        let client_clone = client.clone();
        let tx_clone = tx.clone();
        let recording_state_clone = recording_state.clone();
        let nnnn_tx_clone = nnnn_tx.clone();
        let monitoring_clone = monitoring.clone();

        tokio::spawn(async move {
            let stream_for_log = stream_url.clone();
            if let Err(e) = run_stream_task(
                stream_url,
                client_clone,
                tx_clone,
                recording_state_clone,
                nnnn_tx_clone,
                monitoring_clone,
            )
            .await
            {
                error!(stream = %stream_for_log, "Stream task terminated: {e:?}");
            }
        });
    }

    drop(tx);
    drop(nnnn_tx);

    pending::<()>().await;
    #[allow(unreachable_code)]
    Ok(())
}

async fn run_stream_task(
    stream_url: String,
    client: reqwest::Client,
    tx: TokioSender<(String, String, String, String, Duration, String)>,
    recording_state: Arc<Mutex<Option<RecordingState>>>,
    nnnn_tx: BroadcastSender<()>,
    monitoring: MonitoringHub,
) -> Result<()> {
    let mut last_log_time = Instant::now() - Duration::from_secs(61);
    let mut last_log_time2 = Instant::now() - Duration::from_secs(61);

    loop {
        monitoring.note_connecting(&stream_url);
        if last_log_time.elapsed() > Duration::from_secs(60) {
            info!(stream = %stream_url, "Connecting to Icecast stream");
            last_log_time = Instant::now();
        }

        match client
            .get(&stream_url)
            .header(
                reqwest::header::ACCEPT,
                "audio/*,application/ogg;q=0.9,*/*;q=0.1",
            )
            .header(reqwest::header::CONNECTION, "keep-alive")
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    monitoring.note_error(
                        &stream_url,
                        format!("unexpected status: {}", response.status()),
                    );
                    if last_log_time2.elapsed() > Duration::from_secs(60) {
                        error!(
                            stream = %stream_url,
                            status = %response.status(),
                            "Received non-success status code; retrying"
                        );
                        last_log_time2 = Instant::now();
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                monitoring.note_connected(&stream_url);
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);

                let (byte_tx, byte_rx) = crossbeam_channel::bounded::<Bytes>(256);

                let stream_for_reader = stream_url.clone();
                let monitoring_reader = monitoring.clone();
                tokio::spawn(async move {
                    let mut response = response;

                    let mut last_warn = std::time::Instant::now();

                    loop {
                        match tokio::time::timeout(stream_inactivity_timeout(), response.chunk())
                            .await
                        {
                            Ok(Ok(Some(chunk))) => match byte_tx.try_send(chunk) {
                                Ok(_) => {
                                    monitoring_reader.note_activity(&stream_for_reader);
                                }
                                Err(crossbeam_channel::TrySendError::Full(_)) => {
                                    if last_warn.elapsed() > std::time::Duration::from_secs(30) {
                                        tracing::warn!(stream=%stream_for_reader, "Decoder backpressure: dropping audio chunks to keep socket draining");
                                        last_warn = std::time::Instant::now();
                                    }
                                }
                                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                    break;
                                }
                            },
                            Ok(Ok(None)) => {
                                monitoring_reader
                                    .note_error(&stream_for_reader, "EOF from server".to_string());
                                break;
                            }
                            Ok(Err(e)) => {
                                monitoring_reader.note_error(
                                    &stream_for_reader,
                                    format!("chunk read error: {e}"),
                                );
                                break;
                            }
                            Err(_) => {
                                tracing::warn!(stream=%stream_for_reader, "Audio stream stalled; reconnecting");
                                monitoring_reader
                                    .note_error(&stream_for_reader, "stream stalled".to_string());
                                break;
                            }
                        }
                    }
                });

                let tx_clone = tx.clone();
                let recording_state_clone = recording_state.clone();
                let nnnn_tx_clone = nnnn_tx.clone();
                let stream_for_decode = stream_url.clone();
                let decoding_task = tokio::task::spawn_blocking(move || {
                    let reader = ChannelReader {
                        rx: byte_rx,
                        buffer: Bytes::new(),
                        pos: 0,
                    };
                    let source = ReadOnlySource::new(reader);
                    let mss = MediaSourceStream::new(Box::new(source), Default::default());
                    process_stream(
                        mss,
                        content_type,
                        &tx_clone,
                        &recording_state_clone,
                        &nnnn_tx_clone,
                        &stream_for_decode,
                    )
                });
                if let Err(e) = decoding_task.await? {
                    monitoring.note_error(&stream_url, format!("decode error: {e}"));
                    error!(
                        stream = %stream_url,
                        "Error processing audio stream: {}. Reconnecting...",
                        e
                    );
                }
                monitoring.note_disconnected(&stream_url);
            }
            Err(e) => {
                error!(
                    stream = %stream_url,
                    "Failed to connect to Icecast stream: {}. Retrying...",
                    e
                );
                monitoring.note_error(&stream_url, format!("connect error: {e}"));
                continue;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn process_stream(
    mss: MediaSourceStream,
    content_type: Option<String>,
    tx: &TokioSender<(String, String, String, String, Duration, String)>,
    recording_state: &Arc<Mutex<Option<RecordingState>>>,
    nnnn_tx: &BroadcastSender<()>,
    stream_label: &str,
) -> Result<()> {
    let runtime = tokio::runtime::Handle::current();
    let mut hint = Hint::new();
    if let Some(ct) = content_type {
        if ct.contains("audio/mpeg") {
            hint.with_extension("mp3");
        }
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("Unsupported format")?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("No default track found"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Failed to make decoder")?;
    let mut same_receiver = SameReceiverBuilder::new(TARGET_SAMPLE_RATE).build();
    let mut resampler: Option<SincFixedIn<f32>> = None;
    const CHUNK_SIZE: usize = 2048;
    let mut audio_buffer: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => {
                error!(stream = %stream_label, "Packet error: {}", e);
                break;
            }
        };
        match decoder.decode(&packet) {
            Ok(decoded) => {
                if decoded.frames() == 0 {
                    continue;
                }
                let spec = *decoded.spec();
                let current_resampler = resampler.get_or_insert_with(|| {
                    use rubato::{
                        SincInterpolationParameters, SincInterpolationType, WindowFunction,
                    };
                    info!(
                        stream = %stream_label,
                        "Stream detected with sample rate {}. Resampling to {}.",
                        spec.rate,
                        TARGET_SAMPLE_RATE
                    );
                    SincFixedIn::new(
                        48000.0 / spec.rate as f64,
                        2.0,
                        SincInterpolationParameters {
                            sinc_len: 256,
                            f_cutoff: 0.95,
                            interpolation: SincInterpolationType::Linear,
                            oversampling_factor: 256,
                            window: WindowFunction::BlackmanHarris2,
                        },
                        CHUNK_SIZE,
                        1,
                    )
                    .unwrap()
                });
                let mut mono_samples = vec![0.0f32; decoded.frames()];
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.frames() as u64, spec);
                sample_buf.copy_interleaved_ref(decoded);
                for (i, frame) in sample_buf
                    .samples()
                    .chunks_exact(spec.channels.count())
                    .enumerate()
                {
                    mono_samples[i] = frame.iter().sum::<f32>() / frame.len() as f32;
                }
                audio_buffer.extend_from_slice(&mono_samples);
                while audio_buffer.len() >= CHUNK_SIZE {
                    let chunk_to_process = audio_buffer[..CHUNK_SIZE].to_vec();
                    let resampled = current_resampler.process(&[chunk_to_process], None)?;
                    let samples_f32 = resampled[0].clone();

                    let recording_sender = {
                        let recorder = recording_state.blocking_lock();
                        recorder
                            .as_ref()
                            .filter(|state| state.source_stream == stream_label)
                            .map(|state| state.audio_tx.clone())
                    };

                    if let Some(audio_tx) = recording_sender {
                        if let Err(e) = audio_tx.try_send(samples_f32.clone()) {
                            if let TrySendError::Closed(_) = e {
                                warn!(
                                    stream = %stream_label,
                                    "Recording task channel closed unexpectedly."
                                );
                            }
                        }
                    }

                    for msg in same_receiver.iter_messages(samples_f32) {
                        match msg {
                            SameMessage::StartOfMessage(header) => {
                                let event = header.event_str().to_string();
                                let locations =
                                    header.location_str_iter().collect::<Vec<_>>().join(", ");
                                let originator = header.originator_str().to_string();
                                let raw_header = header.as_str().to_string();
                                let purge_time = header.valid_duration();
                                let std_purge_time =
                                    Duration::from_secs(purge_time.num_seconds().max(0) as u64);
                                if let Err(e) = runtime.block_on(tx.send((
                                    event,
                                    locations,
                                    originator,
                                    raw_header,
                                    std_purge_time,
                                    stream_label.to_string(),
                                ))) {
                                    error!(
                                        stream = %stream_label,
                                        "Failed to send decoded data: {}",
                                        e
                                    );
                                }
                            }
                            SameMessage::EndOfMessage => {
                                info!(stream = %stream_label, "NNNN (End of Message) detected");
                                if let Err(e) = nnnn_tx.send(()) {
                                    error!(
                                        stream = %stream_label,
                                        "Failed to broadcast NNNN signal: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    audio_buffer.drain(..CHUNK_SIZE);
                }
            }
            Err(e) => {
                warn!(stream = %stream_label, "Decode error: {}", e);
            }
        }
    }
    Ok(())
}
