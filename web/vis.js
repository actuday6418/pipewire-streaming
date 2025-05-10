(async () => {
    const serverUrl = 'https://localhost:13345'; // Your WebTransport server
    const sampleRate = 48000;
    const numberOfChannels = 1;
    const frameDurationMs = 5; // From your Rust const

    let audioContext;
    let audioDecoder;
    let transport;
    let connected = false;
    let analyser;
    let dataArray;
    let bufferLength;

    const statusDisplay = document.getElementById('status');
    const connectButton = document.getElementById('connectButton');
    const canvas = document.getElementById('visualizerCanvas');
    const canvasCtx = canvas.getContext('2d');

    // HASH from your original script - specific to your self-signed certificate
    // Generate this for your cert.pem:
    // openssl x509 -in cert.pem -noout -pubkey | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | xxd -p -c 1000 | sed 's/../,&0x&/g' | sed 's/^,//'
    // Or, if you know the hex string, you can convert it to a Uint8Array.
    // Example HASH, replace with your actual one:
    const HASH_HEX = "b4f32e52f6193cc68035f55fbc4f706e345897e0edd51fbce1038e91633855f2"; // Example! Replace!
    const HASH = new Uint8Array(HASH_HEX.match(/.{1,2}/g).map(byte => parseInt(byte, 16)));


    function initAudioAndVisualizer() {
        try {
            console.log("Initializing AudioContext, AnalyserNode, and AudioDecoder...");
            audioContext = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: sampleRate });

            if (audioContext.state === 'suspended') {
                console.log("AudioContext suspended, attempting to resume...");
                audioContext.resume().then(() => console.log("AudioContext resumed."));
            }

            // Setup AnalyserNode
            analyser = audioContext.createAnalyser();
            analyser.fftSize = 2048; // Or 1024, 512. Power of 2. Affects detail.
            bufferLength = analyser.frequencyBinCount; // Always fftSize / 2
            dataArray = new Uint8Array(bufferLength); // For time-domain (waveform) data

            audioDecoder = new AudioDecoder({
                output: handleDecodedChunk,
                error: (e) => console.error("AudioDecoder error:", e.message, e),
            });

            audioDecoder.configure({
                codec: "opus",
                sampleRate: sampleRate,
                numberOfChannels: numberOfChannels,
            });

            console.log("Audio and Visualizer initialized.");
            drawWaveform(); // Start the drawing loop
        } catch (err) {
            console.error("Failed to initialize audio/visualizer:", err);
            statusDisplay.textContent = "Initialization failed.";
            if (audioContext) {
                audioContext.close();
                audioContext = null;
            }
        }
    }

    function handleDecodedChunk(audioData) {
        if (!audioContext || !analyser) {
            console.warn("AudioContext or Analyser not ready, skipping chunk.");
            audioData.close();
            return;
        }
        try {
            // Create an AudioBuffer from the AudioData
            const pcmData = new Float32Array(audioData.allocationSize({ planeIndex: 0 }));
            audioData.copyTo(pcmData, { planeIndex: 0 });

            const audioBuffer = audioContext.createBuffer(
                audioData.numberOfChannels,
                audioData.numberOfFrames,
                audioData.sampleRate
            );
            audioBuffer.copyToChannel(pcmData, 0);

            // Create a new BufferSourceNode for each chunk
            const sourceNode = audioContext.createBufferSource();
            sourceNode.buffer = audioBuffer;

            // Connect source to analyser, but NOT to audioContext.destination
            sourceNode.connect(analyser);
            sourceNode.start(); // "Play" the sound into the analyser (it won't be heard)

            // Clean up AudioData object
            audioData.close();

        } catch (err) {
            console.error("Error processing decoded chunk for visualizer:", err);
        }
    }

    function drawWaveform() {
        requestAnimationFrame(drawWaveform); // Loop the drawing

        if (!analyser || !connected || !canvasCtx) {
             // Clear canvas if not connected or analyser not ready
            if (canvasCtx) {
                canvasCtx.fillStyle = 'rgb(220, 220, 220)';
                canvasCtx.fillRect(0, 0, canvas.width, canvas.height);
                if (!connected && audioContext) { // Draw "Disconnected" message
                    canvasCtx.font = "16px Arial";
                    canvasCtx.fillStyle = "black";
                    canvasCtx.textAlign = "center";
                    canvasCtx.fillText("Disconnected. Press Connect.", canvas.width / 2, canvas.height / 2);
                }
            }
            return;
        }

        analyser.getByteTimeDomainData(dataArray); // Fill dataArray with waveform data

        canvasCtx.fillStyle = 'rgb(0, 0, 0)'; // Background color
        canvasCtx.fillRect(0, 0, canvas.width, canvas.height);

        canvasCtx.lineWidth = 2;
        canvasCtx.strokeStyle = 'rgb(0, 255, 0)'; // Waveform color
        canvasCtx.beginPath();

        const sliceWidth = canvas.width * 1.0 / bufferLength;
        let x = 0;

        for (let i = 0; i < bufferLength; i++) {
            const v = dataArray[i] / 128.0; // dataArray[i] is 0-255. Normalize to 0.0-2.0
            const y = v * canvas.height / 2;

            if (i === 0) {
                canvasCtx.moveTo(x, y);
            } else {
                canvasCtx.lineTo(x, y);
            }
            x += sliceWidth;
        }

        canvasCtx.lineTo(canvas.width, canvas.height / 2); // Draw line to end
        canvasCtx.stroke();
    }


    async function connectAndReceive() {
        if (connected) return;

        statusDisplay.textContent = "Initializing...";
        initAudioAndVisualizer();

        if (!audioContext || !audioDecoder) {
            statusDisplay.textContent = "Audio/Visualizer initialization failed. Cannot proceed.";
            console.error("Audio/Visualizer initialization failed. Cannot proceed.");
            return;
        }

        try {
            statusDisplay.textContent = "Connecting...";
            transport = new WebTransport(serverUrl, {
                serverCertificateHashes: [{ algorithm: "sha-256", value: HASH.buffer }]
            });
            await transport.ready;
            statusDisplay.textContent = "Connected";
            connected = true;
            connectButton.disabled = true;

            const uniStreamReader = transport.incomingUnidirectionalStreams.getReader();
            const { value: stream, done: streamDone } = await uniStreamReader.read();
            uniStreamReader.releaseLock();

            if (streamDone) {
                console.log("No unidirectional stream received.");
                statusDisplay.textContent = "Failed to get stream.";
                return;
            }

            const reader = stream.getReader();

            while (true) {
                const { value, done } = await reader.read();
                if (done) {
                    console.log("Stream closed by server.");
                    statusDisplay.textContent = "Stream closed.";
                    break;
                }
                if (value && value.length > 0) {
                    const chunk = new EncodedAudioChunk({
                        type: 'key', // Assuming all Opus chunks are key frames for simplicity here
                        timestamp: audioContext.currentTime * 1000000, // Microseconds
                        duration: frameDurationMs * 1000, // Microseconds
                        data: value
                    });
                    try {
                        if (audioDecoder.state === "configured") {
                            audioDecoder.decode(chunk);
                        } else {
                            console.warn("Decoder not configured, skipping packet. State: %s", audioDecoder.state);
                        }
                    } catch (decodeError) {
                        console.error("Error during decode call:", decodeError);
                    }
                }
            }

        } catch (error) {
            console.error("WebTransport connection or reading error:", error);
            statusDisplay.textContent = `Error: ${error.message}`;
        } finally {
            connected = false;
            connectButton.disabled = false;
            if (transport) {
                transport.close();
                console.log("WebTransport closed.");
            }
            if (statusDisplay.textContent === "Connected" || statusDisplay.textContent === "Stream closed." ) {
                 statusDisplay.textContent = "Disconnected";
            }
            // Do not close audioContext here if you want to allow reconnections
            // or if you want the visualizer to persist.
            // If you do want to clean it up fully:
            // if (audioContext) {
            //     audioContext.close().then(() => console.log("AudioContext closed."));
            //     audioContext = null;
            //     analyser = null;
            // }
        }
    }

    connectButton.addEventListener('click', () => {
        if (!connected) {
            connectAndReceive();
        }
    });

    // Initial draw (e.g., a blank canvas or "disconnected" message)
    drawWaveform();

})();
