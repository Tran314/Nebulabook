#!/usr/bin/env python3
"""Synthetic, Chinese-language notebook data for real-display smoke screenshots.

No production behavior or personal notebooks are used. This helper only creates
authenticated .nebula v1 input for visual scenarios inside the smoke runner's
disposable XDG_DATA_HOME. These inputs are not evidence of a GUI save: separate
keyboard editing workflows verify actual application writes with the independent
reader. Self-tests check the fixture protocol, not the GUI implementation.
"""
import hmac
import json
import os
import struct
from pathlib import Path
import unittest

SEARCH_QUERY = "Glass Study"
MISSING_QUERY = "no-note-matches-7f2b"
DEMO_TITLE = "留一点空间，给新的想法"
TRASH_TITLE = "暂存的旧方案"


def note(index, title, content, *, pinned=False, deleted=False):
    timestamp = f"2026-10-08T09:{index:02d}:00+00:00"
    return {
        "id": f"00000000-0000-4000-8000-{index:012d}",
        "title": title,
        "content": content,
        "folder_id": None,
        "tag_ids": [],
        "is_pinned": pinned,
        "is_deleted": deleted,
        "created_at": "2026-10-01T08:00:00+00:00",
        "updated_at": timestamp,
        "deleted_at": timestamp if deleted else None,
        "version": 1,
    }


def notebook(empty=False):
    notes = [] if empty else [
        note(9, DEMO_TITLE,
             "把日常里一闪而过的灵感，慢慢写下来。\n\n"
             "今天想完成的三件小事\n"
             "01  整理桌面，让注意力回到眼前\n"
             "02  记录一个值得继续探索的想法\n"
             "03  留出一段不被打扰的阅读时间\n\n"
             "关于这个工作空间\n"
             "柔和的光、清楚的层次，以及恰到好处的留白。"
             "工具安静一些，思考就可以自由一些。\n\n"
             "中文与 English 可以自然地放在同一段文字里。"
             "长句需要在窄窗口中正常换行，文字不应被侧边栏或操作按钮遮住。\n\n"
             "下一步\n回顾本周的记录，再把真正重要的事留在置顶。", pinned=True),
        note(8, "本周计划 · 保持专注", "周一：整理阅读笔记\n周三：复盘项目\n周五：为下周留出空间", pinned=True),
        note(7, "界面灵感 · Glass Study", "Glass Study\n半透明的层次、柔和的边缘、清晰的文字。\n只保留真正有用的动作。"),
        note(6, "读书摘录", "记录值得回看的句子，也记下当时自己的问题。\n阅读不是收集答案，而是扩展思考。"),
        note(5, "周末散步路线", "沿着河岸走一走。\n带上相机，看看傍晚的光落在哪里。"),
        note(4, "一个很长的标题，用来检查笔记列表是否能优雅地截断而不挤压其他内容", "长标题和短预览都应该保持可读。"),
        note(3, "随手记", "咖啡、清单、突然出现的小想法。\n无需整理完美，先记录下来。"),
        note(2, TRASH_TITLE, "这是用于回收站截图的演示内容。\n删除后仍保留正文，需要时可以恢复。", deleted=True),
        note(1, "已完成的清单", "旧清单保留在回收站中。\n这份示例没有任何真实用户数据。", deleted=True),
    ]
    return {"schema_version": 1, "notes": notes, "folders": [], "tags": []}


def seed_nebula(data_home, *, empty=False):
    """Initialize visual input only while the app is stopped; never overwrite it."""
    folder = Path(data_home) / "nebulanotepad"
    folder.mkdir(mode=0o700, parents=True)
    path = folder / "notebook.nebula"
    encoded = encode_nebula(notebook(empty))
    # Set private permissions when creating, not after writing the data.
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(encoded)
    return path


# Independent test-only codec: production uses RustCrypto, never this code.
# Both directions are checked against published vectors and the shared fixture,
# so matching Python encode/decode bugs cannot establish format compatibility.
# Algorithm definitions / known-answer vectors:
# https://www.rfc-editor.org/rfc/rfc8439 (2.3.2, 2.5.2, 2.8)
# https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha-03 (2.2.1)
# This public fixed format key is deliberately not a secret or a password.
FORMAT_KEY = b"Nebulabook public format key v1!"
MAGIC = b"NEBULA\r\n"
HEADER_SIZE = 44
MASK32 = (1 << 32) - 1


def _rounds(state):
    state = state.copy()

    def quarter(a, b, c, d):
        for left, right, shift in ((a, b, 16), (c, d, 12), (a, b, 8), (c, d, 7)):
            state[left] = (state[left] + state[right]) & MASK32
            target = d if left == a else b
            value = state[target] ^ state[left]
            state[target] = ((value << shift) | (value >> (32 - shift))) & MASK32

    for _ in range(10):
        for indices in ((0, 4, 8, 12), (1, 5, 9, 13), (2, 6, 10, 14), (3, 7, 11, 15),
                        (0, 5, 10, 15), (1, 6, 11, 12), (2, 7, 8, 13), (3, 4, 9, 14)):
            quarter(*indices)
    return state


def _key_state(key):
    return list(struct.unpack("<4I", b"expand 32-byte k") + struct.unpack("<8I", key))


def _hchacha(key, nonce):
    state = _rounds(_key_state(key) + list(struct.unpack("<4I", nonce)))
    return struct.pack("<8I", *(state[:4] + state[12:]))


def _chacha_block(key, counter, nonce):
    initial = _key_state(key) + [counter] + list(struct.unpack("<3I", nonce))
    return struct.pack("<16I", *[(a + b) & MASK32 for a, b in zip(initial, _rounds(initial))])


def _poly1305(message, key):
    r = int.from_bytes(key[:16], "little") & 0x0ffffffc0ffffffc0ffffffc0fffffff
    accumulator = 0
    for offset in range(0, len(message), 16):
        block = int.from_bytes(message[offset:offset + 16] + b"\x01", "little")
        accumulator = ((accumulator + block) * r) % ((1 << 130) - 5)
    tag = (accumulator + int.from_bytes(key[16:], "little")) % (1 << 128)
    return tag.to_bytes(16, "little")


def _decrypt_xchacha(key, nonce, aad, ciphertext, tag):
    subkey = _hchacha(key, nonce[:16])
    short_nonce = b"\0" * 4 + nonce[16:]
    poly_key = _chacha_block(subkey, 0, short_nonce)[:32]
    authenticated = (aad + b"\0" * (-len(aad) % 16)
                     + ciphertext + b"\0" * (-len(ciphertext) % 16)
                     + struct.pack("<QQ", len(aad), len(ciphertext)))
    if not hmac.compare_digest(_poly1305(authenticated, poly_key), tag):
        raise ValueError("Disposable .nebula fixture authentication failed")
    plain = bytearray()
    for offset in range(0, len(ciphertext), 64):
        key_stream = _chacha_block(subkey, offset // 64 + 1, short_nonce)
        plain.extend(a ^ b for a, b in zip(ciphertext[offset:offset + 64], key_stream))
    return bytes(plain)


def _encrypt_xchacha(key, nonce, aad, plaintext):
    """Only prepare synthetic visual inputs, never application-save evidence."""
    subkey = _hchacha(key, nonce[:16])
    short_nonce = b"\0" * 4 + nonce[16:]
    ciphertext = bytearray()
    for offset in range(0, len(plaintext), 64):
        key_stream = _chacha_block(subkey, offset // 64 + 1, short_nonce)
        ciphertext.extend(a ^ b for a, b in zip(plaintext[offset:offset + 64], key_stream))
    ciphertext = bytes(ciphertext)
    poly_key = _chacha_block(subkey, 0, short_nonce)[:32]
    authenticated = (aad + b"\0" * (-len(aad) % 16)
                     + ciphertext + b"\0" * (-len(ciphertext) % 16)
                     + struct.pack("<QQ", len(aad), len(ciphertext)))
    return ciphertext, _poly1305(authenticated, poly_key)


def encode_nebula(data, *, nonce=None):
    """Encode current v1 fixtures with the existing, explicit null source field."""
    nonce = os.urandom(24) if nonce is None else nonce
    if len(nonce) != 24:
        raise ValueError("A .nebula nonce must contain 24 bytes")
    plaintext = json.dumps({"notebook": data, "legacy_source_sha256": None},
                           ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(plaintext) + HEADER_SIZE + 16 > 1024 * 1024:
        raise ValueError("Disposable smoke notebook unexpectedly exceeds 1 MiB")
    header = MAGIC + struct.pack("<HH", 1, 0) + nonce + struct.pack("<Q", len(plaintext) + 16)
    ciphertext, tag = _encrypt_xchacha(FORMAT_KEY, nonce, header, plaintext)
    return header + ciphertext + tag


def decode_nebula(encoded):
    """Authenticate before decoding; limited to small, synthetic smoke data."""
    if not HEADER_SIZE + 16 <= len(encoded) <= 1024 * 1024:
        raise ValueError("Invalid disposable .nebula fixture size")
    if encoded[:8] != MAGIC:
        raise ValueError("Expected a .nebula file, not plaintext JSON")
    version, flags = struct.unpack("<HH", encoded[8:12])
    if version != 1 or flags != 0:
        raise ValueError("Unsupported .nebula fixture version/flags")
    length, = struct.unpack("<Q", encoded[36:44])
    if length != len(encoded) - HEADER_SIZE:
        raise ValueError("Invalid .nebula fixture length")
    plaintext = _decrypt_xchacha(FORMAT_KEY, encoded[12:36], encoded[:44],
                                encoded[44:-16], encoded[-16:])
    payload = json.loads(plaintext)
    if set(payload) != {"notebook", "legacy_source_sha256"}:
        raise ValueError("Unexpected .nebula fixture payload fields")
    legacy = payload["legacy_source_sha256"]
    if legacy is not None and (not isinstance(legacy, str) or len(legacy) != 64
                               or any(c not in "0123456789abcdef" for c in legacy)):
        raise ValueError("Invalid legacy source digest")
    return payload["notebook"]


def read_notebook(path, *, isolated_root):
    """Only permit files beneath the current temporary smoke data directory."""
    path = Path(path).resolve()
    if not path.is_relative_to(Path(isolated_root).resolve()):
        raise ValueError("Refusing to read outside the disposable smoke directory")
    if path.stat().st_size > 1024 * 1024:
        raise ValueError("Disposable smoke notebook unexpectedly exceeds 1 MiB")
    return decode_nebula(path.read_bytes())


class DecoderTests(unittest.TestCase):
    def test_chacha_rfc8439_block_vector(self):
        block = _chacha_block(bytes(range(32)), 1, bytes.fromhex("000000090000004a00000000"))
        self.assertEqual(block.hex(),
                         "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4e"
                         "d2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e")

    def test_hchacha_draft_vector(self):
        subkey = _hchacha(bytes(range(32)), bytes.fromhex("000000090000004a0000000031415927"))
        self.assertEqual(subkey.hex(), "82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc")

    def test_poly1305_rfc8439_vector(self):
        key = bytes.fromhex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
        self.assertEqual(_poly1305(b"Cryptographic Forum Research Group", key).hex(),
                         "a8061dc1305136c6c22b8baf0c0127a9")

    def test_xchacha_aead_draft_vector_and_tamper(self):
        # Full, independent AEAD vector from draft-irtf-cfrg-xchacha-03 A.3.1.
        key = bytes(range(0x80, 0xa0))
        nonce = bytes(range(0x40, 0x58))
        aad = bytes.fromhex("50515253c0c1c2c3c4c5c6c7")
        ciphertext = bytes.fromhex(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb"
            "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452"
            "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9"
            "21f9664c97637da9768812f615c68b13b52e")
        tag = bytes.fromhex("c0875924c1c7987947deafd8780acf49")
        expected = bytes.fromhex(
            "4c616469657320616e642047656e746c656d656e206f662074686520636c6173"
            "73206f66202739393a204966204920636f756c64206f6666657220796f75206f"
            "6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73"
            "637265656e20776f756c642062652069742e")
        self.assertEqual(_decrypt_xchacha(key, nonce, aad, ciphertext, tag), expected)
        self.assertEqual(_encrypt_xchacha(key, nonce, aad, expected), (ciphertext, tag))
        for corrupted in ((key, nonce, aad + b"x", ciphertext, tag),
                          (key, nonce, aad, ciphertext[:-1] + b"x", tag),
                          (key, nonce, aad, ciphertext, bytes(16))):
            with self.assertRaisesRegex(ValueError, "authentication failed"):
                _decrypt_xchacha(*corrupted)

    def test_shared_rust_format_fixture_and_tamper(self):
        fixture = Path(__file__).resolve().parent.parent / "tests/fixtures/empty-v1.nebula"
        encoded = fixture.read_bytes()
        self.assertEqual(len(encoded), 155)
        self.assertEqual(encoded[12:36], bytes(range(24)))
        self.assertEqual(encode_nebula(notebook(empty=True), nonce=bytes(range(24))), encoded)
        self.assertEqual(decode_nebula(encoded), notebook(empty=True))
        for index in (0, 8, 10, 12, 36, 44, len(encoded) - 1):
            changed = bytearray(encoded)
            changed[index] ^= 1
            with self.subTest(index=index), self.assertRaises(ValueError):
                decode_nebula(bytes(changed))
        for changed in (encoded[:-1], encoded + b"\n"):
            with self.assertRaises(ValueError):
                decode_nebula(changed)

    def test_rejects_json_and_wrong_framing(self):
        for encoded in (b"", json.dumps(notebook()).encode(), MAGIC + b"\0" * 64):
            with self.assertRaises(ValueError):
                decode_nebula(encoded)

    def test_reader_rejects_non_smoke_path(self):
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "outside the disposable"):
                read_notebook(Path(temporary).parent / "notebook.nebula", isolated_root=temporary)


class FixtureTests(unittest.TestCase):
    def test_schema_one_and_unique_ids(self):
        data = notebook()
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(len({n["id"] for n in data["notes"]}), len(data["notes"]))
        self.assertEqual(data["folders"], [])
        self.assertEqual(data["tags"], [])

    def test_demo_covers_pins_search_and_trash(self):
        notes = notebook()["notes"]
        self.assertEqual(sum(n["is_pinned"] and not n["is_deleted"] for n in notes), 2)
        self.assertEqual(sum(n["is_deleted"] for n in notes), 2)
        matches = [n for n in notes if SEARCH_QUERY.lower() in (n["title"] + n["content"]).lower()]
        self.assertEqual(len(matches), 1)
        self.assertTrue(all(MISSING_QUERY not in n["title"] + n["content"] for n in notes))
        for n in notes:
            self.assertEqual(n["is_deleted"], n["deleted_at"] is not None)
            self.assertGreater(n["version"], 0)
            self.assertIsNone(n["folder_id"])
            self.assertEqual(n["tag_ids"], [])

    def test_empty_and_deterministic(self):
        self.assertEqual(notebook(empty=True)["notes"], [])
        self.assertEqual(notebook(), notebook())

    def test_private_seed_never_overwrites(self):
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            path = seed_nebula(temporary)
            self.assertEqual(path.name, "notebook.nebula")
            self.assertEqual(read_notebook(path, isolated_root=temporary), notebook())
            self.assertFalse(path.with_suffix(".json").exists())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(path.parent.stat().st_mode & 0o777, 0o700)
            before = path.read_bytes()
            with self.assertRaises(FileExistsError):
                seed_nebula(temporary, empty=True)
            self.assertEqual(path.read_bytes(), before)

    def test_native_fixture_shape_null_source_and_fresh_nonce(self):
        for empty in (False, True):
            expected = notebook(empty)
            first, second = encode_nebula(expected), encode_nebula(expected)
            self.assertNotEqual(first[12:36], second[12:36])
            self.assertEqual(decode_nebula(first), expected)
            self.assertEqual(decode_nebula(second), expected)
            plain = _decrypt_xchacha(FORMAT_KEY, first[12:36], first[:44],
                                    first[44:-16], first[-16:])
            self.assertEqual(json.loads(plain), {"notebook": expected, "legacy_source_sha256": None})
            self.assertNotIn(DEMO_TITLE.encode("utf-8"), first)

    def test_writer_rejects_invalid_nonce_and_oversized_input(self):
        for nonce in (b"", bytes(23), bytes(25)):
            with self.subTest(nonce_length=len(nonce)), self.assertRaises(ValueError):
                encode_nebula(notebook(), nonce=nonce)
        with self.assertRaisesRegex(ValueError, "exceeds 1 MiB"):
            encode_nebula({"synthetic_oversized_value": "x" * 1024 * 1024})


if __name__ == "__main__":
    unittest.main()
