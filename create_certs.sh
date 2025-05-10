# Replace `local_ip`
local_ip="192.168.1.3"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
           -keyout key.pem -out cert.pem -days 13 \
           -subj "/CN=localhost" \
           -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:${local_ip}"
