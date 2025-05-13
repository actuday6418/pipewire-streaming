use anyhow::{Context, Result, bail};
use rodio::{OutputStream, Sink, buffer::SamplesBuffer};
use std::thread;
use std::time::Duration;
use wtransport::ClientConfig;
use wtransport::tls::Sha256Digest;

const SERVER_URL: &str = "https://localhost:13345";
const SAMPLE_RATE: u32 = 48_000;
const OPUS_CHANNELS: opus::Channels = opus::Channels::Mono;
const PLAYBACK_CHANNELS: u16 = 1;
const OPUS_FRAME_MS_SERVER: u32 = 10;
const SAMPLES_PER_FRAME_EXPECTED: usize = (SAMPLE_RATE * OPUS_FRAME_MS_SERVER / 1000) as usize;

const SERVER_CERT_HASH_BYTES: [u8; 32] = [
    13, 168, 113, 2, 213, 136, 124, 10, 80, 208, 200, 56, 29, 68, 119, 16, 194, 119, 112, 219, 4,
    102, 187, 137, 91, 248, 119, 10, 167, 127, 119, 240,
];

const MAX_PCM_SAMPLES_PER_FRAME: usize = (48_000 * 120) / 1000;

fn playback_thread(
    pcm_receiver: crossbeam_channel::Receiver<Vec<i16>>,
    sample_rate: u32,
    channels: u16,
) -> Result<()> {
    let (_stream, stream_handle) =
        OutputStream::try_default().context("Failed to get default audio output stream")?;
    let sink = Sink::try_new(&stream_handle).context("Failed to create audio sink")?;

    for pcm_data in pcm_receiver {
        if pcm_data.is_empty() {
            println!("[PlaybackThread] Received empty PCM data, skipping.");
            continue;
        }
        println!(
            "[PlaybackThread] Received {} PCM samples. Queue size: {}",
            pcm_data.len(),
            sink.len()
        );

        let source = SamplesBuffer::new(channels, sample_rate, pcm_data);
        sink.append(source);
        sink.play();
    }
    sink.sleep_until_end();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Connecting to: {}", SERVER_URL);
    let server_cert_hash = Sha256Digest::new(SERVER_CERT_HASH_BYTES);
    let config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    let endpoint = wtransport::Endpoint::client(config)
        .context("Failed to create WebTransport client endpoint")?;
    let connection = endpoint
        .connect(SERVER_URL)
        .await
        .context(format!("Failed to connect to server at {}", SERVER_URL))?;
    println!("Waiting for incoming unidirectional stream...");
    let mut stream_reader = connection
        .accept_uni()
        .await
        .context("Failed to accept unidirectional stream from server")?;

    let (pcm_sender, pcm_receiver) = crossbeam_channel::unbounded::<Vec<i16>>();

    let playback_handle = thread::spawn(move || {
        if let Err(e) = playback_thread(pcm_receiver, SAMPLE_RATE, PLAYBACK_CHANNELS) {
            eprintln!("[PlaybackThread] Error: {:?}", e);
        }
    });
    let mut opus_decoder =
        opus::Decoder::new(SAMPLE_RATE, OPUS_CHANNELS).context("Failed to create Opus decoder")?;
    let mut pcm_out_buffer = vec![0i16; MAX_PCM_SAMPLES_PER_FRAME];
    let mut pcm_in_buffer = vec![0u8; MAX_PCM_SAMPLES_PER_FRAME];

    let mut packet_count = 0;
    println!("[NetworkRead] Reading Opus packets from stream...");

    loop {
        if let Ok(Some(no)) = stream_reader.read(&mut pcm_in_buffer).await {
            packet_count += 1;
            match opus_decoder.decode(&pcm_in_buffer[..no], &mut pcm_out_buffer, false) {
                Ok(decoded_sample_count) => {
                    if decoded_sample_count > 0 {
                        if decoded_sample_count != SAMPLES_PER_FRAME_EXPECTED {
                            println!(
                                "[NetworkRead] WARN: Decoded {} samples, expected {}.",
                                decoded_sample_count, SAMPLES_PER_FRAME_EXPECTED
                            );
                        }
                        let pcm_to_send = pcm_out_buffer[..decoded_sample_count].to_vec();
                        if pcm_sender.send(pcm_to_send).is_err() {
                            println!(
                                "[NetworkRead] Playback thread seems to have exited. Stopping."
                            );
                            break;
                        }
                    } else {
                        println!(
                            "[NetworkRead] Opus decoder returned 0 samples for packet {}.",
                            packet_count
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[NetworkRead] Opus decoding error for packet {}: {:?}. Skipping packet.",
                        packet_count, e
                    );
                }
            }
        }
    }
    drop(pcm_sender);
    if playback_handle.join().is_err() {
        eprintln!("Playback thread panicked.");
    }
    Ok(())
}
