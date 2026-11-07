import json
import os
from pathlib import Path
import runpy
import signal
import struct
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def integer(value):
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return result


def blob(value):
    return integer(len(value)) + value


def request(operation, path=b"-", data=b"", append=False, binary=False):
    payload = bytearray([34]) + integer(2**64 - 1) + blob(path) + integer(operation)
    if operation == 1:
        payload += bytes([append]) + blob(data)
    if operation == 2:
        payload += bytes([binary])
    return struct.pack("<I", len(payload) + 4) + b"\0\0\x76\0" + payload


def responses(data):
    result = []
    while data:
        size = struct.unpack_from("<I", data)[0] + 4
        frame, data = data[:size], data[size:]
        offset = 8

        def uint():
            nonlocal offset
            value = shift = 0
            while True:
                byte = frame[offset]
                offset += 1
                value |= (byte & 127) << shift
                if byte < 128:
                    return value
                shift += 7

        def value():
            nonlocal offset
            length = uint()
            value = frame[offset:offset + length]
            offset += length
            return value

        assert uint() == 35
        assert uint() == 2**64 - 1
        payload = value()
        error = value().decode() if uint() else None
        assert offset == len(frame)
        result.append((payload, error))
    return result


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    probe = TARGET / "client-file-io-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/client_file_io_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    checked = 0
    with tempfile.TemporaryDirectory(prefix="zz-client-files-") as directory:
        directory = Path(directory)
        result = directory / "result.bin"
        serial = 0

        def invoke(frames, payload=b"", restore=False, closed=False, stdin=None):
            nonlocal serial, checked
            paths = []
            for frame in frames:
                path = directory / f"request-{serial}.bin"
                serial += 1
                path.write_bytes(frame)
                paths.append(str(path))
            command = [str(probe), str(result)] + (["--restore-signal"] if restore else []) + paths
            completed = subprocess.run(command, input=payload if stdin is None else None, stdin=stdin, capture_output=True, preexec_fn=(lambda: os.close(0)) if closed else None, timeout=20)
            if closed:
                assert completed.returncode == 1, completed
                assert completed.stderr == b"[err] evsig_cb: recv: Bad file descriptor\n", completed.stderr
                checked += 1
                return []
            assert completed.returncode == 0, completed.stderr
            checked += 1
            return responses(result.read_bytes())

        assert invoke([request(2, binary=True), request(2, binary=True)], b"\0\xffbinary\n") == [(b"\0\xffbinary\n", None), (b"", "Bad file descriptor")]
        assert invoke([request(2)], "héllo 世界\n".encode(), restore=True) == [("héllo 世界\n".encode(), None)]
        assert invoke([request(2)], b"\xff") == [(b"", "could not read standard input: stream did not contain valid UTF-8")]
        assert invoke([request(2, binary=True)], b"x" * 1048576) == [(b"x" * 1048576, None)]
        assert invoke([request(2, binary=True)], b"x" * 1048577) == [(b"", "standard input exceeds 1048576 bytes")]
        chunks = directory / "chunks.bin"
        chunks.write_bytes(b"x" * 32769)
        with chunks.open("rb") as source:
            assert invoke([request(3)] * 5, stdin=source) == [(b"x" * 16384, None), (b"x" * 16384, None), (b"x", None), (b"", None), (b"", "Bad file descriptor")]
        invoke([request(2)], closed=True)
        descriptor = os.open(directory, os.O_RDONLY)
        try:
            assert invoke([request(2)], stdin=descriptor) == [(b"", "Input/output error")]
        finally:
            os.close(descriptor)
        path = directory / "file 世界.bin"
        encoded = os.fsencode(path)
        assert invoke([request(1, encoded, b"\0\xfffirst"), request(1, encoded, b"\x80last", append=True), request(0, encoded)]) == [(b"", None), (b"", None), (b"\0\xfffirst\x80last", None)]
        assert invoke([request(1, encoded, b"short"), request(0, encoded)]) == [(b"", None), (b"short", None)]
        assert invoke([request(0, os.fsencode(directory / "missing"))]) == [(b"", "No such file or directory")]
        assert invoke([request(0, encoded + b"\0ignored")]) == [(b"", "file name contained an unexpected NUL byte")]
        path.write_bytes(b"b" * (32 * 1024 * 1024))
        assert invoke([request(0, encoded)]) == [(b"b" * (32 * 1024 * 1024), None)]
        with path.open("ab") as output:
            output.write(b"x")
        assert invoke([request(0, encoded)]) == [(b"", "client file exceeds 33554432 bytes")]
        blocking = directory / "blocking.bin"
        blocking.write_bytes(request(2, binary=True))
        process = subprocess.Popen([str(probe), str(result), str(blocking)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(process.communicate())
                if Path(f"/proc/{process.pid}/wchan").read_text().strip() == "anon_pipe_read":
                    break
                time.sleep(0.01)
            else:
                raise AssertionError("stdin reader did not block")
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=5) == 0
            checked += 1
        finally:
            if process.poll() is None:
                process.kill()
            process.communicate()
    report = {"passed": True, "groups": checked, "scope": "raw files, append/truncation/errors, full/chunked stdin, spent state, byte bounds, closed descriptors, signal restoration and blocked termination"}
    (TARGET / "client-file-io.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
