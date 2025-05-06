use compress::spawn_compress_thread;
use http::spawn_http_thread;
use libspa::pod;
use libspa::utils::Direction;
use pipewire as pw;
use tokio::sync::watch;
use webtransport::spawn_webtransport_thread;

mod compress;
mod http;
mod webtransport;

const SAMPLE_RATE: u32 = 48_000;
const OPUS_FRAME_MS: u32 = 5;
const SAMPLES_PER_FRAME: u32 = (SAMPLE_RATE * OPUS_FRAME_MS) / 1000;
const WEBTRANSPORT_PORT: u16 = 13345;
const HTTP_PORT: u16 = 13346;

struct SinkData {
    sender: crossbeam_channel::Sender<Vec<i16>>,
}

fn main() {
    let (raw_packet_tx, raw_packet_rx) = crossbeam_channel::unbounded();
    let (compressed_packet_tx, compressed_packet_rx) = watch::channel(Vec::new());
    let _webtransport_handle = spawn_webtransport_thread(compressed_packet_rx, WEBTRANSPORT_PORT);
    let _worker_handle = spawn_compress_thread(raw_packet_rx, compressed_packet_tx);
    let _http_handle = spawn_http_thread();

    pw::init();
    let main_loop = pw::main_loop::MainLoop::new(None).expect("Couldn't create PipeWire MainLoop");
    let context = pw::context::Context::new(&main_loop).expect("Couldn't create PipeWire Context");
    let core = context.connect(None).expect("Couldn't connect to PipeWire");
    let stream = pw::stream::Stream::new(
        &core,
        "fake-audio-sink",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::NODE_NAME => "fake-speaker",
            *pw::keys::NODE_DESCRIPTION => "Fake Speaker",
            *pw::keys::NODE_LATENCY => "500/48000",
        },
    )
    .expect("Couldn't create PipeWire stream");

    let sink_data = SinkData {
        sender: raw_packet_tx.clone(),
    };
    let _listener = stream
        .add_local_listener_with_user_data(sink_data)
        .process(move |stream, user_data| {
            stream.dequeue_buffer().map(|mut buffer| {
                let datas = buffer.datas_mut();
                if !datas.is_empty() {
                    let maybe_data = datas[0].data();
                    if let Some(data) = maybe_data {
                        let packet_bytes: Vec<i16> = data
                            .chunks_exact(2)
                            .map(|chunk| {
                                let bytes: [u8; 2] =
                                    chunk.try_into().expect("Chunk size should be 2");
                                i16::from_le_bytes(bytes)
                            })
                            .collect();
                        user_data.sender.send(packet_bytes).unwrap()
                    }
                }
            });
        })
        .register()
        .expect("Couldn't register stream listener");

    let mut audio_info = libspa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(libspa::param::audio::AudioFormat::S16P);
    audio_info.set_channels(1);
    audio_info.set_rate(SAMPLE_RATE);

    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .unwrap()
    .0
    .into_inner();
    let mut params = [pod::Pod::from_bytes(&values).unwrap()];

    stream
        .connect(
            Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .expect("Failed to connect stream");
    main_loop.run();
    stream.disconnect().expect("Couldn't disconnect stream");
}
