"""Generate isolated TLS/release trust for fixture artifacts, never production keys."""

import argparse
import json
import subprocess
from pathlib import Path


def generate(output, port=49651):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True, mode=0o700)
    certificate, private = output / "localhost.pem", output / "localhost-key.pem"
    ca, ca_key = output / "ca.pem", output / "ca-key.pem"
    signing, public = output / "update-key.pem", output / "update-public-key.hex"
    subprocess.run(
        [
            "openssl",
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "7",
            "-subj",
            "/CN=Jam E2E isolated root",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-addext",
            "keyUsage=critical,keyCertSign,cRLSign",
            "-keyout",
            str(ca_key),
            "-out",
            str(ca),
        ],
        check=True,
        capture_output=True,
        timeout=30,
    )
    request = output / "localhost.csr"
    extensions = output / "localhost.ext"
    extensions.write_text(
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\n"
        "subjectAltName=DNS:localhost,IP:127.0.0.1\n"
    )
    subprocess.run(
        [
            "openssl",
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-keyout",
            str(private),
            "-out",
            str(request),
        ],
        check=True,
        capture_output=True,
        timeout=30,
    )
    subprocess.run(
        [
            "openssl",
            "x509",
            "-req",
            "-in",
            str(request),
            "-CA",
            str(ca),
            "-CAkey",
            str(ca_key),
            "-CAcreateserial",
            "-days",
            "7",
            "-extfile",
            str(extensions),
            "-out",
            str(certificate),
        ],
        check=True,
        capture_output=True,
        timeout=30,
    )
    subprocess.run(
        ["openssl", "genpkey", "-algorithm", "ED25519", "-out", str(signing)],
        check=True,
        capture_output=True,
        timeout=30,
    )
    der = subprocess.run(
        ["openssl", "pkey", "-in", str(signing), "-pubout", "-outform", "DER"],
        check=True,
        capture_output=True,
        timeout=30,
    ).stdout
    if der[:12].hex() != "302a300506032b6570032100" or len(der) != 44:
        raise ValueError("Unexpected Ed25519 public key encoding")
    public.write_text(der[12:].hex() + "\n")
    for path in (private, signing, ca_key):
        path.chmod(0o600)
    config = {
        "origin": f"https://localhost:{port}",
        "certificate": certificate.name,
        "ca_certificate": ca.name,
        "tls_key": private.name,
        "signing_key": signing.name,
        "public_key": public.name,
    }
    (output / "services.json").write_text(json.dumps(config, indent=2) + "\n")
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--port", type=int, default=49651)
    args = parser.parse_args()
    print(json.dumps(generate(args.output, args.port), indent=2))


if __name__ == "__main__":
    main()
