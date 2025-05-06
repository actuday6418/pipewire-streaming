use crate::{SAMPLE_RATE, SAMPLES_PER_FRAME};
use opus::{Application, Channels, Encoder};
use std::{thread::JoinHandle, time::Duration};
use tokio::sync::watch;

pub fn spawn_compress_thread(
    rx: crossbeam_channel::Receiver<Vec<i16>>,
    tx: watch::Sender<Vec<u8>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut opus_encoder =
            Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Audio).unwrap();
        let mut count: usize = 0;
        let mut compressed_count: usize = 0;
        let ticker = crossbeam_channel::tick(Duration::from_secs(1));

        loop {
            crossbeam_channel::select! {
                recv(rx) -> msg => match msg {
                    Ok(mut samples) => {
                        let mut output_buffer = [0; 8192];
                        count += samples.len();
                        samples.resize(SAMPLES_PER_FRAME as usize, 0);
                        let compressed_this_frame = opus_encoder.encode(&samples, &mut output_buffer).expect("Couldn't encode!");
                        compressed_count += compressed_this_frame;
                        tx.send(output_buffer[..compressed_this_frame].to_vec()).unwrap();
                    },
                    Err(_) => {
                        break;
                    }
                },
                recv(ticker) -> _ => {
                    // println!("Bytes/sec: {}, Compressed/sec: {}", count, compressed_count);
                    count = 0;
                    compressed_count = 0;
                }
            }
        }
    })
}
