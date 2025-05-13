use crate::{SAMPLE_RATE, SAMPLES_PER_FRAME};
use opus::{Application, Channels, Encoder};
use ringbuf::traits::{Consumer, Observer, Producer, RingBuffer};
use std::{thread::JoinHandle, time::Duration};
use tokio::sync::broadcast;

pub fn spawn_compress_thread(
    rx: crossbeam_channel::Receiver<Vec<i16>>,
    tx: broadcast::Sender<Vec<u8>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut opus_encoder =
            Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Audio).unwrap();
        let mut count: usize = 0;
        let mut compressed_count: usize = 0;
        let ticker = crossbeam_channel::tick(Duration::from_secs(1));
        let mut buff = ringbuf::rb::local::LocalRb::new(SAMPLES_PER_FRAME as usize * 5);
        let mut output_buffer = [0; 8192];
        let mut input_buffer = [0; SAMPLES_PER_FRAME as usize];

        loop {
            crossbeam_channel::select! {
                recv(rx) -> msg => match msg {
                    Ok(samples) => {
                        count += samples.len();
                        buff.push_slice(&samples);
                        while buff.occupied_len() >= SAMPLES_PER_FRAME as usize {
                            let len = buff.pop_slice(&mut input_buffer);
                            input_buffer[len..].fill(0);
                            let compressed_this_frame = opus_encoder.encode(&input_buffer, &mut output_buffer).expect("Couldn't encode!");
                            compressed_count += compressed_this_frame;
                            tx.send(output_buffer[..compressed_this_frame].to_vec()).unwrap();
                        }
                    },
                    Err(_) => {
                        break;
                    }
                },
                recv(ticker) -> _ => {
                    println!("Bytes/sec: {}, Compressed/sec: {}", count, compressed_count);
                    count = 0;
                    compressed_count = 0;
                }
            }
        }
    })
}
