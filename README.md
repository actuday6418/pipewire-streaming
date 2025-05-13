# Usage
* Generate certificates if your local IP changes and you want to connect from another device on the LAN. Remember to update IP address in `create_certs.sh`. `sh create_cers.sh`
* Pick one of the clients:
  * Simple JS - Js client with a crinkling audio artefacts in bursts - seems to be a buffer underrun. Just copy `clients/simple-js/web` over to the repo root.
  * Visualizer JS - Js client only presenting an audio visualizer. No audio.
  * Rust WASM - WASM client, has the same audio artefact as the Js client, but latency seems to be lower. Build with `wasm-pack build --target web --out-dir web/pkg` while in `clients/rust-wasm` and then copy `clients/rust-wasm/web` over to the repo root.
  * Rust native - Perfect audio quality, obviously won't run in the browser.
* Run the server with `cargo r --release`
* If required, wire some audio into the server (named "Fake Speaker") using a PipeWire GUI like `Helvum`.
* Connect from the client (if using a web client, access it using URL/QR printed by the server.)
