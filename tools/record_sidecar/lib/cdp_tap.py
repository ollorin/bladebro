#!/usr/bin/env python3
"""Logging TCP proxy in front of Chrome's debug port (stdlib only; run with `python3 -I`).

    cdp_tap.py LISTEN_PORT CHROME_PORT LOGFILE
    cdp_tap.py --self-test

Why: the off-mode comparison (07-03, SC3) needs the exact CDP methods a bladebro
CLI session sends, against the pre-phase binary and the new one. Python
`websockets` is not installed here, so this is plain sockets + RFC 6455 framing.

- HTTP: requests are forwarded to 127.0.0.1:CHROME_PORT. `/json*` response bodies get
  `127.0.0.1:CHROME_PORT` rewritten to `127.0.0.1:LISTEN_PORT` (Content-Length fixed) so
  the client's webSocketDebuggerUrl comes back through this proxy. Each request line is
  logged as `HTTP <METHOD> <path>`.
- WebSocket: after a 101 upgrade, client->server frames are unmasked, fragments are
  reassembled and each text message's JSON `method` is logged as `WS <method>`.
  server->client bytes pass through untouched.
- LOGFILE is appended line by line and flushed per line.
"""
import json
import socket
import struct
import sys
import threading

CHROME_PORT = 0
LISTEN_PORT = 0
LOG = None
LOCK = threading.Lock()


def log(line):
    with LOCK:
        LOG.write(line + "\n")
        LOG.flush()


class FrameParser:
    """Feed client->server bytes; get complete (opcode, payload) messages back."""

    def __init__(self):
        self.buf = b""
        self.frag_op = None
        self.frag = b""

    def feed(self, data):
        self.buf += data
        out = []
        while True:
            if len(self.buf) < 2:
                break
            b0, b1 = self.buf[0], self.buf[1]
            fin, op = b0 & 0x80, b0 & 0x0F
            masked, ln = b1 & 0x80, b1 & 0x7F
            pos = 2
            if ln == 126:
                if len(self.buf) < 4:
                    break
                ln = struct.unpack(">H", self.buf[2:4])[0]
                pos = 4
            elif ln == 127:
                if len(self.buf) < 10:
                    break
                ln = struct.unpack(">Q", self.buf[2:10])[0]
                pos = 10
            mask = b""
            if masked:
                if len(self.buf) < pos + 4:
                    break
                mask = self.buf[pos : pos + 4]
                pos += 4
            if len(self.buf) < pos + ln:
                break
            payload = self.buf[pos : pos + ln]
            self.buf = self.buf[pos + ln :]
            if masked:
                payload = bytes(c ^ mask[i % 4] for i, c in enumerate(payload))
            if op >= 0x8:  # control frame: never fragmented
                out.append((op, payload))
                continue
            if op != 0:
                self.frag_op, self.frag = op, payload
            else:
                self.frag += payload
            if fin:
                out.append((self.frag_op, self.frag))
                self.frag_op, self.frag = None, b""
        return out


def log_message(op, payload):
    if op != 1:
        return
    try:
        m = json.loads(payload.decode("utf-8", "replace")).get("method")
    except Exception:
        return
    if m:
        log("WS " + m)


def read_headers(sock, pre=b""):
    data = pre
    while b"\r\n\r\n" not in data:
        chunk = sock.recv(65536)
        if not chunk:
            return None, data
        data += chunk
    i = data.index(b"\r\n\r\n") + 4
    return data[:i], data[i:]


def header_value(head, name):
    for line in head.split(b"\r\n")[1:]:
        k, _, v = line.partition(b":")
        if k.strip().lower() == name:
            return v.strip().decode("latin1")
    return None


def rewrite_json_response(head, body):
    """Rewrite the chrome port to the listen port and fix Content-Length."""
    new = body.replace(
        ("127.0.0.1:%d" % CHROME_PORT).encode(), ("127.0.0.1:%d" % LISTEN_PORT).encode()
    ).replace(
        ("localhost:%d" % CHROME_PORT).encode(), ("127.0.0.1:%d" % LISTEN_PORT).encode()
    )
    lines = head.split(b"\r\n")
    out = []
    for ln in lines:
        if ln.lower().startswith(b"content-length:"):
            ln = b"Content-Length: " + str(len(new)).encode()
        out.append(ln)
    return b"\r\n".join(out), new


def pump_plain(src, dst):
    try:
        while True:
            d = src.recv(65536)
            if not d:
                break
            dst.sendall(d)
    except OSError:
        pass
    for s in (src, dst):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass


def pump_client_frames(src, dst, parser):
    try:
        while True:
            d = src.recv(65536)
            if not d:
                break
            for op, payload in parser.feed(d):
                log_message(op, payload)
            dst.sendall(d)
    except OSError:
        pass
    for s in (src, dst):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass


def handle(client):
    up = socket.create_connection(("127.0.0.1", CHROME_PORT))
    leftover = b""
    try:
        while True:
            head, leftover = read_headers(client, leftover)
            if head is None:
                return
            first = head.split(b"\r\n")[0].decode("latin1")
            log("HTTP " + first.rsplit(" ", 1)[0])
            upgrade = (header_value(head, b"upgrade") or "").lower() == "websocket"
            clen = int(header_value(head, b"content-length") or 0)
            while len(leftover) < clen:
                more = client.recv(65536)
                if not more:
                    return
                leftover += more
            body, leftover = leftover[:clen], leftover[clen:]
            fixed = []
            for ln in head.split(b"\r\n"):
                if ln.lower().startswith(b"host:"):
                    ln = ("Host: 127.0.0.1:%d" % CHROME_PORT).encode()
                fixed.append(ln)
            up.sendall(b"\r\n".join(fixed) + body)
            rhead, rrest = read_headers(up)
            if rhead is None:
                return
            status = rhead.split(b"\r\n")[0].split(b" ")[1] if b" " in rhead else b""
            if upgrade and status == b"101":
                client.sendall(rhead + rrest)
                parser = FrameParser()
                for op, payload in parser.feed(leftover):
                    log_message(op, payload)
                if leftover:
                    up.sendall(leftover)
                t = threading.Thread(target=pump_plain, args=(up, client), daemon=True)
                t.start()
                pump_client_frames(client, up, parser)
                t.join(timeout=2)
                return
            rlen = int(header_value(rhead, b"content-length") or 0)
            while len(rrest) < rlen:
                more = up.recv(65536)
                if not more:
                    break
                rrest += more
            rbody = rrest[:rlen]
            if first.split(" ")[1].startswith("/json"):
                rhead, rbody = rewrite_json_response(rhead, rbody)
            client.sendall(rhead + rbody)
    except OSError:
        pass
    finally:
        for s in (client, up):
            try:
                s.close()
            except OSError:
                pass


def serve():
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", LISTEN_PORT))
    srv.listen(16)
    while True:
        c, _ = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


def masked_frame(payload, op=1, fin=True, key=b"\x01\x02\x03\x04"):
    n = len(payload)
    b0 = (0x80 if fin else 0) | op
    if n < 126:
        hdr = bytes([b0, 0x80 | n])
    elif n < 65536:
        hdr = bytes([b0, 0x80 | 126]) + struct.pack(">H", n)
    else:
        hdr = bytes([b0, 0x80 | 127]) + struct.pack(">Q", n)
    return hdr + key + bytes(c ^ key[i % 4] for i, c in enumerate(payload))


def self_test():
    global CHROME_PORT, LISTEN_PORT
    for size in (5, 300, 70000):
        body = json.dumps({"id": 1, "method": "X.y", "params": {"p": "a" * max(0, size - 40)}})
        body = body.encode()
        got = FrameParser().feed(masked_frame(body))
        assert got == [(1, body)], "size %d: %r" % (size, got[:1])
    # fragmented message, split across feeds and frames
    msg = json.dumps({"id": 2, "method": "Frag.test"}).encode()
    p = FrameParser()
    f1, f2 = masked_frame(msg[:7], op=1, fin=False), masked_frame(msg[7:], op=0, fin=True)
    blob = f1 + f2
    got = p.feed(blob[:5]) + p.feed(blob[5:20]) + p.feed(blob[20:])
    assert got == [(1, msg)], got
    # ping between fragments stays a separate control message
    p = FrameParser()
    got = p.feed(masked_frame(msg[:3], 1, False) + masked_frame(b"hi", 9) + masked_frame(msg[3:], 0))
    assert got == [(9, b"hi"), (1, msg)], got
    # /json body rewrite with Content-Length
    CHROME_PORT, LISTEN_PORT = 9222, 19222
    body = b'{"webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/page/AB"}'
    head = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d" % len(body)
    h2, b2 = rewrite_json_response(head, body)
    assert b"127.0.0.1:19222" in b2 and b"127.0.0.1:9222" not in b2
    assert header_value(h2 + b"\r\n\r\n", b"content-length") == str(len(b2))
    print("cdp_tap self-test ok")


def main():
    global CHROME_PORT, LISTEN_PORT, LOG
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    LISTEN_PORT, CHROME_PORT = int(sys.argv[1]), int(sys.argv[2])
    LOG = open(sys.argv[3], "a")
    serve()


if __name__ == "__main__":
    main()
