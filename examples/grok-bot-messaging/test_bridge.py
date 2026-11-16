import os
from pathlib import Path
import runpy
import tempfile
import threading
import unittest
from unittest.mock import patch


class BridgeTest(unittest.TestCase):
    def test_same_plain_text_returns_one_native_reply(self):
        with tempfile.TemporaryDirectory() as state_dir, patch.dict(os.environ, {
            'GROK_NATIVE_PAIRED_BOT_ID': 'grok-bot:fixture:00000000-0000-0000-0000-000000000001',
            'GROK_NATIVE_CODEX_THREAD_ID': '00000000-0000-0000-0000-000000000002',
            'GROK_NATIVE_STATE_DIR': state_dir,
            'GROK_NATIVE_KEY': 'connector-fixture',
            'GROK_NATIVE_PAIR_PROOF': 'pair-fixture-secret',
        }), patch('http.server.ThreadingHTTPServer'):
            old_umask = os.umask(0o077)
            try:
                bridge = runpy.run_path(str(Path(__file__).with_name('server.py')))
            finally:
                os.umask(old_umask)
            exchange = bridge['exchange']
            message = '请用一句话介绍这个会话。'
            calls = []

            def native_call(name, args, caller):
                calls.append((name, args, caller))
                if name == 'send_message':
                    return {'id': 'm-fixture', 'state': 'queued'}
                return {'state': 'done', 'answer': '这是一个测试会话。'}

            args = {'arguments': {'message': message, 'pairing_code': 'pair-fixture-secret'}}
            with patch.dict(exchange.__globals__, {'native_call': native_call}):
                self.assertTrue(exchange({'arguments': {'message': message, 'pairing_code': 'wrong'}})[1])
                self.assertFalse(calls)
                self.assertFalse(exchange(args)[1])
                for _ in range(200):
                    reply, error = exchange(args)
                    if '这是一个测试会话。' in reply:
                        break
                    threading.Event().wait(0.01)
                self.assertFalse(error)
                self.assertIn('这是一个测试会话。', reply)
                self.assertEqual([name for name, _, _ in calls], ['send_message', 'get_message'])
                self.assertTrue(calls[0][1]['text'].endswith(message))
                self.assertEqual(calls[0][2]['client'], 'grok-bot')
