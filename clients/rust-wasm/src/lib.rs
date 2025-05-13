use js_sys::{Array, Object, Reflect, Uint8Array};
use std::cell::RefCell;
use std::panic;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextOptions, AudioData, AudioDataCopyToOptions, AudioDecoder,
    AudioDecoderConfig, AudioDecoderInit, EncodedAudioChunk, EncodedAudioChunkInit,
    EncodedAudioChunkType, HtmlButtonElement, HtmlParagraphElement, ReadableStreamDefaultReader,
    WebTransport, WebTransportOptions, console,
};

const SERVER_CERT_HASH_BYTES: [u8; 32] = [
    13, 168, 113, 2, 213, 136, 124, 10, 80, 208, 200, 56, 29, 68, 119, 16, 194, 119, 112, 219, 4,
    102, 187, 137, 91, 248, 119, 10, 167, 127, 119, 240,
];
const SAMPLE_RATE: f32 = 48000.0;
const NUMBER_OF_CHANNELS: u32 = 1;
const FRAME_DURATION_MS: u32 = 5;

thread_local! {
    static AUDIO_CONTEXT: RefCell<Option<AudioContext>> = RefCell::new(None);
    static AUDIO_DECODER: RefCell<Option<AudioDecoder>> = RefCell::new(None);
    static NEXT_PLAY_TIME: RefCell<f64> = RefCell::new(0.0);
    static RECEIVED_CHUNK_COUNT: RefCell<u64> = RefCell::new(0);
    static STATUS_ELEMENT: RefCell<Option<HtmlParagraphElement>> = RefCell::new(None);
    static TRANSPORT: RefCell<Option<WebTransport>> = RefCell::new(None);
}

#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    console::log_1(&"Rust WASM Audio Client loaded.".into());

    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    let connect_button = document
        .get_element_by_id("connectButton")
        .expect("should have #connectButton on the page")
        .dyn_into::<HtmlButtonElement>()?;

    STATUS_ELEMENT.with(|cell| {
        *cell.borrow_mut() = Some(
            document
                .get_element_by_id("status")
                .expect("should have #status on the page")
                .dyn_into::<HtmlParagraphElement>()
                .unwrap(),
        );
    });

    let closure = Closure::wrap(Box::new(move || {
        console::log_1(&"Connect button clicked (Rust)".into());
        update_status("Connecting...");
        wasm_bindgen_futures::spawn_local(async {
            if let Err(e) = connect_and_receive().await {
                console::error_1(&format!("Connection error: {:?}", e).into());
                update_status(&format!("Error: {:?}", e));
            }
        });
    }) as Box<dyn FnMut()>);

    connect_button.set_onclick(Some(closure.as_ref().unchecked_ref()));
    closure.forget(); // To keep the closure alive

    Ok(())
}

fn update_status(message: &str) {
    STATUS_ELEMENT.with(|cell| {
        if let Some(status_el) = cell.borrow().as_ref() {
            status_el.set_text_content(Some(message));
        }
    });
    console::log_1(&message.into());
}

fn init_audio() -> Result<(), JsValue> {
    console::log_1(&"Initializing AudioContext and AudioDecoder (Rust)...".into());

    let context_options = AudioContextOptions::new();
    context_options.set_sample_rate(SAMPLE_RATE);
    let audio_context = AudioContext::new_with_context_options(&context_options)?;

    if audio_context.state() == web_sys::AudioContextState::Suspended {
        console::log_1(&"AudioContext suspended, attempting to resume...".into());
        let _ = audio_context.resume()?;
    }

    let handle_decoded_chunk_closure = Closure::wrap(Box::new(move |audio_data: AudioData| {
        if let Err(e) = handle_decoded_chunk_internal(audio_data) {
            console::error_1(&format!("Error in handle_decoded_chunk_internal: {:?}", e).into());
            update_status(&format!("Decode handle error: {:?}", e));
        }
    }) as Box<dyn FnMut(AudioData)>);

    let decoder_error_closure = Closure::wrap(Box::new(move |e: JsValue| {
        console::error_1(&"AudioDecoder error (Rust):".into());
        console::error_1(&e);
        update_status(&format!("Decoder Error: {:?}", e));
    }) as Box<dyn FnMut(JsValue)>);

    let decoder_init = AudioDecoderInit::new(
        decoder_error_closure.as_ref().unchecked_ref(),
        handle_decoded_chunk_closure.as_ref().unchecked_ref(),
    );
    handle_decoded_chunk_closure.forget();
    decoder_error_closure.forget();

    let audio_decoder = AudioDecoder::new(&decoder_init)?;

    let decoder_config = AudioDecoderConfig::new("opus", NUMBER_OF_CHANNELS, SAMPLE_RATE as u32);
    audio_decoder.configure(&decoder_config)?;

    AUDIO_CONTEXT.with(|cell| *cell.borrow_mut() = Some(audio_context.clone()));
    AUDIO_DECODER.with(|cell| *cell.borrow_mut() = Some(audio_decoder));
    NEXT_PLAY_TIME.with(|cell| *cell.borrow_mut() = audio_context.current_time());
    RECEIVED_CHUNK_COUNT.with(|cell| *cell.borrow_mut() = 0);

    Ok(())
}

fn handle_decoded_chunk_internal(audio_data: AudioData) -> Result<(), JsValue> {
    let audio_context = AUDIO_CONTEXT.with(|cell| {
        cell.borrow()
            .clone()
            .ok_or_else(|| JsValue::from_str("AudioContext not initialized"))
    })?;

    let copy_to_options = AudioDataCopyToOptions::new(0);

    let allocation_size = audio_data.allocation_size(&copy_to_options)?;
    let mut pcm_data = vec![0; 10240];

    audio_data.copy_to_with_u8_slice(&mut pcm_data, &copy_to_options)?;
    let audio_buffer = audio_context.create_buffer(
        audio_data.number_of_channels(),
        audio_data.number_of_frames(),
        audio_data.sample_rate(),
    )?;
    let float_buffer = unsafe {
        std::slice::from_raw_parts(
            pcm_data[..allocation_size as usize * 4].as_ptr() as *const f32,
            allocation_size as usize,
        )
    };
    audio_buffer.copy_to_channel(&float_buffer, 0)?;

    let source_node = audio_context.create_buffer_source()?;
    source_node.set_buffer(Some(&audio_buffer));
    source_node.connect_with_audio_node(&audio_context.destination())?;

    let current_audio_context_time = audio_context.current_time();
    let next_play_time_global = NEXT_PLAY_TIME.with(|cell| *cell.borrow());

    let start_at;
    if next_play_time_global < current_audio_context_time {
        start_at = current_audio_context_time + 0.005;
    } else {
        start_at = next_play_time_global;
    }

    source_node.start_with_when(start_at)?;

    let updated_next_play_time_global = start_at + audio_buffer.duration();
    NEXT_PLAY_TIME.with(|cell| *cell.borrow_mut() = updated_next_play_time_global);

    audio_data.close();
    Ok(())
}

async fn connect_and_receive() -> Result<(), JsValue> {
    init_audio()?;

    let audio_context_opt = AUDIO_CONTEXT.with(|cell| cell.borrow().clone());
    let audio_decoder_opt = AUDIO_DECODER.with(|cell| cell.borrow().clone());

    if audio_context_opt.is_none() || audio_decoder_opt.is_none() {
        update_status("Audio initialization failed. Cannot proceed.");
        return Err("Audio init failed".into());
    }
    let audio_decoder = audio_decoder_opt.unwrap();

    let window = web_sys::window().expect("no global `window` exists");
    let location = window.location();
    let hostname = location.hostname()?;
    let server_url = format!("https://{}:13345", hostname);
    update_status(&format!("Connecting to {}...", server_url));

    let cert_hash_js_array = Array::new();
    let hash_obj = Object::new();
    Reflect::set(
        &hash_obj,
        &JsValue::from_str("algorithm"),
        &JsValue::from_str("sha-256"),
    )?;
    let cert_hash_buffer = Uint8Array::from(&SERVER_CERT_HASH_BYTES[..]).buffer();
    Reflect::set(
        &hash_obj,
        &JsValue::from_str("value"),
        &JsValue::from(cert_hash_buffer),
    )?;
    cert_hash_js_array.push(&hash_obj);

    let transport_options = WebTransportOptions::new();
    transport_options.set_server_certificate_hashes(&cert_hash_js_array);

    let transport = WebTransport::new_with_options(&server_url, &transport_options)?;
    TRANSPORT.with(|cell| *cell.borrow_mut() = Some(transport.clone()));

    JsFuture::from(transport.ready()).await?;
    update_status("Connected (Rust)");
    update_status("Waiting for server to open a unidirectional stream...");
    let incoming_uni_streams_readable: web_sys::ReadableStream =
        transport.incoming_unidirectional_streams();

    let incoming_uni_streams_reader_obj: js_sys::Object =
        incoming_uni_streams_readable.get_reader();

    let incoming_uni_streams_reader: web_sys::ReadableStreamDefaultReader =
        incoming_uni_streams_reader_obj.dyn_into::<web_sys::ReadableStreamDefaultReader>()?;

    let first_stream_read_result_js = JsFuture::from(incoming_uni_streams_reader.read()).await?;
    let first_stream_read_result_obj = first_stream_read_result_js.dyn_into::<Object>()?;

    let is_done_receiving_streams = Reflect::get(&first_stream_read_result_obj, &"done".into())?
        .as_bool()
        .ok_or_else(|| JsValue::from_str("Failed to read 'done' property from stream result"))?;

    if is_done_receiving_streams {
        update_status("Server closed connection or no unidirectional streams were opened.");
        transport.close();
        return Err(JsValue::from_str(
            "No incoming unidirectional stream received from server.",
        ));
    }

    let stream_value = Reflect::get(&first_stream_read_result_obj, &"value".into())?
        .dyn_into::<web_sys::ReadableStream>()?;
    update_status("Received incoming unidirectional stream. Reading data...");

    let reader = stream_value
        .get_reader()
        .dyn_into::<ReadableStreamDefaultReader>()?;

    loop {
        let result_js = JsFuture::from(reader.read()).await?;
        let result_obj = result_js.dyn_into::<Object>()?;

        let done = Reflect::get(&result_obj, &"done".into())?
            .as_bool()
            .unwrap_or(true);
        if done {
            update_status("Stream closed by server (Rust).");
            break;
        }

        let value_js = Reflect::get(&result_obj, &"value".into())?;
        if value_js.is_undefined() || value_js.is_null() {
            continue;
        }
        let value_uint8_array = value_js.dyn_into::<Uint8Array>()?;

        if value_uint8_array.length() > 0 {
            let current_timestamp_us = RECEIVED_CHUNK_COUNT.with(|cell| {
                let mut count = cell.borrow_mut();
                let ts = *count * (FRAME_DURATION_MS as u64) * 1000;
                *count += 1;
                ts
            });

            let chunk_init = EncodedAudioChunkInit::new(
                &value_uint8_array.into(),
                current_timestamp_us as f64,
                EncodedAudioChunkType::Key,
            );
            chunk_init.set_duration(FRAME_DURATION_MS as f64 * 1000.0);

            let chunk = EncodedAudioChunk::new(&chunk_init)?;

            if audio_decoder.state() == web_sys::CodecState::Configured {
                audio_decoder.decode(&chunk)?;
            } else {
                console::warn_1(
                    &format!(
                        "Decoder not configured, skipping packet. State: {:?}",
                        audio_decoder.state()
                    )
                    .into(),
                );
            }
        }
    }

    TRANSPORT.with(|cell| {
        if let Some(transport) = cell.borrow_mut().take() {
            transport.close();
        }
    });
    AUDIO_CONTEXT.with(|cell| {
        if let Some(ctx) = cell.borrow_mut().take() {
            let _ = ctx.close();
        }
    });
    update_status("Disconnected (Rust)");
    Ok(())
}
