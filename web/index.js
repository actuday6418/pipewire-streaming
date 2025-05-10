(async () => {
        const serverUrl = 'https://localhost:13345';
        const sampleRate = 48000;
        const numberOfChannels = 1;
        const frameDurationMs = 5;

        let audioContext;
        let audioDecoder;
        let transport;
        let nextPlayTime = 0;
        let connected = false;
        const statusDisplay = document.getElementById('status');

        function initAudio() {
             try {
                console.log("Initializing AudioContext and AudioDecoder...");
                audioContext = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: sampleRate });

                if (audioContext.state === 'suspended') {
                    console.log("AudioContext suspended, attempting to resume...");
                    audioContext.resume().then(() => console.log("AudioContext resumed."));
                }

                audioDecoder = new AudioDecoder({
                    output: handleDecodedChunk,
                    error: (e) => console.error("AudioDecoder error:", e.message, e),
                });

                audioDecoder.configure({
                    codec: "opus",
                    sampleRate: sampleRate,
                    numberOfChannels: numberOfChannels,
                });
                nextPlayTime = audioContext.currentTime;
            } catch (err) {
                console.error("Failed to initialize audio:", err);
            }
        }

        function handleDecodedChunk(audioData) {
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
                sourceNode.connect(audioContext.destination);

                const startAt = Math.max(nextPlayTime, audioContext.currentTime);
                sourceNode.start(startAt);

                nextPlayTime = startAt + audioBuffer.duration;
                audioData.close();
            } catch (err) {
                console.error("Error processing decoded chunk:", err);
            }
        }

         async function connectAndReceive() {
            const HASH = new Uint8Array([203, 161, 236, 157, 162, 69, 127, 21, 203, 108, 16, 39, 61, 52, 160, 238, 48, 62, 101, 119, 169, 3, 216, 17, 236, 38, 206, 11, 53, 98, 168, 132]);
            initAudio();
            if (!audioContext || !audioDecoder) {
                 statusDisplay.textContent = "Audio initialization failed. Cannot proceed.";
                console.error("Audio initialization failed. Cannot proceed.");
                return;
            }
            try {
                transport = new WebTransport(`https://${location.hostname}:13345`, { serverCertificateHashes: [{ algorithm: "sha-256", value: HASH.buffer }]});
                await transport.ready;
                statusDisplay.textContent = "Connected";
                connected = true;

                const uniStreamReader = transport.incomingUnidirectionalStreams.getReader();
                const { value: stream, done: streamDone } = await uniStreamReader.read();
                uniStreamReader.releaseLock();
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
                                console.warn("Decoder not configured, skipping packet: %s", audioDecoder.state);
                            }

                        } catch(decodeError) {
                            console.error("Error during decode call:", decodeError);
                        }

                    }
                }

            } catch (error) {
                console.error("WebTransport connection or reading error:", error);
            } finally {
                if (transport) {
                    transport.close();
                    connected = false;
                     statusDisplay.textContent = "Disconnected";
                }
                if (audioContext) {
                    audioContext.close().then(() => console.log("AudioContext closed."));
                }
            }
        }
      document.getElementById('connectButton').addEventListener('click', () => {
        if (connected) {
          return;
        }
        connectAndReceive();
      });

    })();
