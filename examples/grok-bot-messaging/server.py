import hashlib
import hmac
import json
import os
import secrets
import sqlite3
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from native import CODEX_THREAD_ID, PAIRED_CALLER, NativeError, native_call

CONNECTOR_KEY = os.environ['GROK_NATIVE_KEY'].encode()
PAIR_PROOF = os.environ['GROK_NATIVE_PAIR_PROOF'].encode()
if min(len(CONNECTOR_KEY), len(PAIR_PROOF)) < 16:
    raise ValueError('Use separate random connector and pairing credentials of at least 16 bytes.')
DB = Path(os.environ['GROK_NATIVE_STATE_DIR']).expanduser().resolve() / 'requests.sqlite'
os.umask(0o077)
DB.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
SEND_GATE = threading.Lock()
ANSWERS = {}
READING = set()
if not CODEX_THREAD_ID or not PAIRED_CALLER:
    raise RuntimeError('Configure one trusted Bot and one Codex test thread before startup.')
with sqlite3.connect(DB) as db:
    db.execute("CREATE TABLE IF NOT EXISTS text_requests (digest TEXT PRIMARY KEY, id TEXT, state TEXT NOT NULL)")

def update(digest, message_id, state):
    with sqlite3.connect(DB) as db:
        db.execute('UPDATE text_requests SET id=?,state=? WHERE digest=?', (message_id, state, digest))

def poll_reply(digest, message_id):
    try:
        for _ in range(60):
            try:
                status = native_call('get_message', {'id': message_id}, PAIRED_CALLER)
            except NativeError:
                time.sleep(2)
                continue
            state = status.get('state')
            if state == 'done':
                answer = status.get('answer')
                state = 'done' if isinstance(answer, str) and answer.strip() else 'no reply'
                if state == 'done':
                    with SEND_GATE:
                        ANSWERS[digest] = answer
                update(digest, message_id, state)
                print(json.dumps({'event': 'native_reply', 'digest': digest, 'id': message_id, 'state': state}), flush=True)
                return
            if state in {'refused', 'expired', 'uncertain', 'failed'}:
                update(digest, message_id, state)
                return
            time.sleep(2)
    finally:
        with SEND_GATE:
            READING.discard(digest)

def send_and_poll(digest, message):
    try:
        sent = native_call('send_message', {
            'to': CODEX_THREAD_ID,
            'text': (
                '[Grok Bot]\n'
                'Your final reply will be returned to the paired Grok Bot chat.\n\n' + message
            ),
            'reply_hint': False,
        }, PAIRED_CALLER)
    except NativeError:
        update(digest, None, 'uncertain')
        with SEND_GATE:
            READING.discard(digest)
        return
    message_id, state = sent.get('id'), sent.get('state', 'uncertain')
    update(digest, message_id, state)
    print(json.dumps({'event': 'native_send', 'digest': digest, 'id': message_id, 'state': state}), flush=True)
    if message_id and state in {'queued', 'started', 'waiting', 'taken', 'done'}:
        poll_reply(digest, message_id)
    else:
        with SEND_GATE:
            READING.discard(digest)

def exchange(params):
    args = params.get('arguments', {})
    if not isinstance(args, dict):
        return 'Malformed arguments.', True
    proof = args.get('pairing_code')
    message = args.get('message')
    if not isinstance(proof, str) or not secrets.compare_digest(proof.encode(), PAIR_PROOF):
        return 'This request does not match the paired Bot chat.', True
    if not isinstance(message, str) or not message.strip() or len(message) > 4000:
        return '请输入 1 到 4000 字的消息正文。', True
    request_key = json.dumps([PAIRED_CALLER['sessionId'], CODEX_THREAD_ID, message]).encode()
    digest = hmac.new(CONNECTOR_KEY, request_key, hashlib.sha256).hexdigest()
    with SEND_GATE:
        with sqlite3.connect(DB) as db:
            row = db.execute('SELECT id,state FROM text_requests WHERE digest=?', (digest,)).fetchone()
            if row is None:
                db.execute("INSERT INTO text_requests VALUES (?,NULL,'sending')", (digest,))
        if row is None:
            READING.add(digest)
            threading.Thread(target=send_and_poll, args=(digest, message), daemon=True).start()
            return '消息已受理。请等几秒，用同一工具和完全相同的正文查询 Codex 回复。', False
        message_id, state = row
        if digest in ANSWERS:
            return f'Agent HUD {message_id} · Codex 原文：\n{ANSWERS[digest]}', False
        if message_id and digest not in READING and state in {'queued', 'started', 'waiting', 'taken', 'done'}:
            READING.add(digest)
            threading.Thread(target=poll_reply, args=(digest, message_id), daemon=True).start()
    if state in {'queued', 'started', 'waiting', 'taken', 'done', 'sending'}:
        return f'Agent HUD {message_id or "正在提交"} · 回复尚未返回。请用完全相同的正文再次查询，无需重复投递。', False
    return f'Agent HUD {message_id or ""} · 投递状态：{state}。', True

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_error(405)

    def do_POST(self):
        if self.path != '/mcp':
            self.send_error(404)
            return
        if not secrets.compare_digest(self.headers.get("X-HUD-Test-Key", "").encode(), CONNECTOR_KEY):
            self.send_error(403)
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 32768:
                self.send_error(413)
                return
            request = json.loads(self.rfile.read(length))
            method = request['method']
            params = request.get('params', {})
            if not isinstance(method, str) or not isinstance(params, dict):
                raise ValueError('Malformed request')
        except (ValueError, KeyError, TypeError):
            self.send_error(400)
            return
        if 'id' not in request:
            self.send_response(202)
            self.end_headers()
            return
        reply = {'jsonrpc': '2.0', 'id': request['id']}
        if method == 'initialize':
            version = params.get('protocolVersion')
            reply['result'] = {
                'protocolVersion': version if version in {'2025-03-26','2025-06-18','2025-11-25'} else '2025-11-25',
                'capabilities': {'tools': {}},
                'serverInfo': {'name': 'agent-hud-paired-grok', 'version': '1.1.0'},
            }
        elif method == 'ping':
            reply['result'] = {}
        elif method == 'tools/list':
            reply['result'] = {'tools': [{
                'name': 'paired_codex_exchange',
                'description': 'Send a plain-text message to the paired Codex test chat. Call again with exactly the same text to read its reply without duplicate delivery.',
                'inputSchema': {'type': 'object', 'properties': {
                    'message': {'type': 'string', 'description': 'Exact plain-text message body from the user, up to 4000 characters.'},
                    'pairing_code': {'type': 'string', 'description': 'Temporary pairing code shared in the paired Bot chat.'}
                }, 'required': ['message','pairing_code'], 'additionalProperties': False},
            }]}
        elif method == 'tools/call' and params.get('name') == 'paired_codex_exchange':
            result, error = exchange(params)
            reply['result'] = {'isError': error, 'content': [{'type': 'text', 'text': result}]}
        else:
            reply['error'] = {'code': -32601, 'message': 'Method not available'}
        if method != 'tools/call':
            print(method, flush=True)
        payload = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass

with sqlite3.connect(DB) as db:
    db.execute("UPDATE text_requests SET state='uncertain' WHERE state='sending' AND id IS NULL")
ThreadingHTTPServer(('127.0.0.1', 18766), Handler).serve_forever()
