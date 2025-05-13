(async () => {
    const serverUrl = `https://${location.hostname}:13345`;
    const sampleRate = 48000;
    const numberOfChannels = 1;
    const frameDurationMs = 5;

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

    const HASH = new Uint8Array([13, 168, 113, 2, 213, 136, 124, 10, 80, 208, 200, 56, 29, 68, 119, 16, 194, 119, 112, 219, 4, 102, 187, 137, 91, 248, 119, 10, 167, 127, 119, 240]);


    function initAudioAndVisualizer() {
        try {
            audioContext = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: sampleRate });

            analyser = audioContext.createAnalyser();
            analyser.fftSize = 2048;
            bufferLength = analyser.frequencyBinCount;
            dataArray = new Uint8Array(bufferLength);

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
            drawWaveform();
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
            const pcmData = new Float32Array(audioData.allocationSize({ planeIndex: 0 }));
            audioData.copyTo(pcmData, { planeIndex: 0 });

            const audioBuffer = audioContext.createBuffer(
                audioData.numberOfChannels,
                audioData.numberOfFrames,
                audioData.sampleRate
            );
            audioBuffer.copyToChannel(pcmData, 0);

            const sourceNode = audioContext.createBufferSource();
            sourceNode.buffer = audioBuffer;

            sourceNode.connect(analyser);
            sourceNode.start();

            audioData.close();

        } catch (err) {
            console.error("Error processing decoded chunk for visualizer:", err);
        }
    }

    function drawWaveform() {
        requestAnimationFrame(drawWaveform);

        if (!analyser || !connected || !canvasCtx) {
            if (canvasCtx) {
                canvasCtx.fillStyle = 'rgb(220, 220, 220)';
                canvasCtx.fillRect(0, 0, canvas.width, canvas.height);
                if (!connected && audioContext) {
                    canvasCtx.font = "16px Arial";
                    canvasCtx.fillStyle = "black";
                    canvasCtx.textAlign = "center";
                    canvasCtx.fillText("Disconnected. Press Connect.", canvas.width / 2, canvas.height / 2);
                }
            }
            return;
        }

        analyser.getByteTimeDomainData(dataArray);

        canvasCtx.fillStyle = 'rgb(0, 0, 0)';
        canvasCtx.fillRect(0, 0, canvas.width, canvas.height);

        canvasCtx.lineWidth = 2;
        canvasCtx.strokeStyle = 'rgb(0, 255, 0)';
        canvasCtx.beginPath();

        const sliceWidth = canvas.width * 1.0 / bufferLength;
        let x = 0;

        for (let i = 0; i < bufferLength; i++) {
            const v = dataArray[i] / 128.0;
            const y = v * canvas.height / 2;

            if (i === 0) {
                canvasCtx.moveTo(x, y);
            } else {
                canvasCtx.lineTo(x, y);
            }
            x += sliceWidth;
        }

        canvasCtx.lineTo(canvas.width, canvas.height / 2);
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
                        type: 'key',
                        timestamp: audioContext.currentTime * 1000000,
                        duration: frameDurationMs * 1000,
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
        }
    }

    connectButton.addEventListener('click', () => {
        if (!connected) {
            connectAndReceive();
        }
    });

    drawWaveform();

})();
