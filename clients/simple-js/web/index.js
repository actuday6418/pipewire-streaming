const SERVER_CERT_HASH_BYTES = [
    13, 168, 113, 2, 213, 136, 124, 10, 80, 208, 200, 56, 29, 68, 119, 16,
    194, 119, 112, 219, 4, 102, 187, 137, 91, 248, 119, 10, 167, 127, 119, 240,
];
const SAMPLE_RATE = 48000;
const NUMBER_OF_CHANNELS = 1;
const FRAME_DURATION_MS = 5;

let audioContext = null;
let audioDecoder = null;
let nextPlayTime = 0.0;
let receivedChunkCount = 0;
let transport = null;

const connectButton = document.getElementById('connectButton');
const statusElement = document.getElementById('status');

function updateStatus(message) {
    console.log(message);
    statusElement.textContent = message;
}

async function initAudio() {
    try {
        audioContext = new AudioContext({ sampleRate: SAMPLE_RATE });
        if (audioContext.state === 'suspended') {
            updateStatus("AudioContext suspended, attempting to resume...");
            await audioContext.resume();
        }
        updateStatus(`AudioContext state: ${audioContext.state}, sample rate: ${audioContext.sampleRate}`);

        audioDecoder = new AudioDecoder({
            output: handleDecodedChunk,
            error: (e) => {
                console.error("AudioDecoder error (JS):", e);
                updateStatus(`Decoder Error: ${e.message}`);
            },
        });

        audioDecoder.configure({
            codec: 'opus',
            sampleRate: SAMPLE_RATE,
            numberOfChannels: NUMBER_OF_CHANNELS,
        });
        
        nextPlayTime = audioContext.currentTime;
        receivedChunkCount = 0;
        updateStatus("Audio initialized.");
    } catch (e) {
        console.error("Audio initialization failed:", e);
        updateStatus(`Audio init error: ${e.message}`);
        throw e;
    }
}

function handleDecodedChunk(audioData) {
    if (!audioContext || audioContext.state === 'closed') {
        console.warn("AudioContext closed, cannot play decoded chunk.");
        audioData.close();
        return;
    }

    const audioBuffer = audioContext.createBuffer(
        audioData.numberOfChannels,
        audioData.numberOfFrames,
        audioData.sampleRate
    );

    for (let i = 0; i < audioData.numberOfChannels; i++) {
        const planeData = new Float32Array(audioData.allocationSize({ planeIndex: i }) / 4);
        audioData.copyTo(planeData, { planeIndex: i, frameOffset: 0, frameCount: audioData.numberOfFrames });
        audioBuffer.copyToChannel(planeData, i, 0);
    }
    
    const sourceNode = audioContext.createBufferSource();
    sourceNode.buffer = audioBuffer;
    sourceNode.connect(audioContext.destination);

    const currentTime = audioContext.currentTime;
    const scheduleTime = (nextPlayTime < currentTime) ? currentTime + 0.005 : nextPlayTime;

    sourceNode.start(scheduleTime);
    nextPlayTime = scheduleTime + audioBuffer.duration;
    
    audioData.close();
}

async function connectAndReceive() {
    updateStatus("Connect button clicked (JS)");
    try {
        await initAudio();

        const serverUrl = `https://${window.location.hostname}:13345`;
        updateStatus(`Connecting to ${serverUrl}...`);

        const serverCertificateHashes = [{
            algorithm: "sha-256",
            value: new Uint8Array(SERVER_CERT_HASH_BYTES).buffer
        }];

        transport = new WebTransport(serverUrl, { serverCertificateHashes });
        await transport.ready;

        const uniStreamsReader = transport.incomingUnidirectionalStreams.getReader();
        const { value: stream, done: noStreams } = await uniStreamsReader.read();
        uniStreamsReader.releaseLock();

        if (noStreams || !stream) {
            updateStatus("Server closed connection or no unidirectional streams were opened.");
            if(transport) transport.close();
            return;
        }
        updateStatus("Received incoming unidirectional stream. Reading data...");

        const reader = stream.getReader();
        while (true) {
            const { value, done } = await reader.read();
            if (done) {
                break;
            }

            if (value && value.byteLength > 0) {
                const timestamp = receivedChunkCount * FRAME_DURATION_MS * 1000;
                receivedChunkCount++;

                const chunk = new EncodedAudioChunk({
                    type: 'key',
                    timestamp: timestamp,
                    duration: FRAME_DURATION_MS * 1000,
                    data: value
                });
                
                if (audioDecoder && audioDecoder.state === 'configured') {
                    audioDecoder.decode(chunk);
                } else {
                    console.warn(`Decoder not configured or null, skipping packet. State: ${audioDecoder?.state}`);
                }
            }
        }
    } catch (e) {
        console.error("Connection error (JS):", e);
        updateStatus(`Error: ${e.message || e}`);
    }
}

connectButton.addEventListener('click', connectAndReceive);
updateStatus("JS Audio Client loaded. Click 'Connect' to start.");
