"""Tests for hexz.checkpoint — tensor-aware checkpoint save/load."""

import os

import pytest

torch = pytest.importorskip("torch")

import hexz  # noqa: E402
from hexz.checkpoint import load, manifest, save  # noqa: E402


# ═══════════════════════════════════════════════════════════════════════════════
#  Basic roundtrip
# ═══════════════════════════════════════════════════════════════════════════════


class TestSaveLoadRoundtrip:
    def test_save_load_roundtrip(self, test_dir):
        """Save 3 tensors, load all, verify torch.allclose for each."""
        state = {
            "weight": torch.randn(64, 32),
            "bias": torch.randn(32),
            "embedding": torch.randn(100, 16),
        }
        path = os.path.join(test_dir, "roundtrip.hxz")
        save(state, path)

        restored = load(path)

        assert set(restored.keys()) == set(state.keys())
        for key in state:
            assert torch.allclose(state[key], restored[key]), f"Mismatch on {key}"

    def test_empty_state_dict(self, test_dir):
        """Save {}, load, verify {}."""
        path = os.path.join(test_dir, "empty_ckpt.hxz")
        save({}, path)

        restored = load(path)
        assert restored == {}

    def test_non_contiguous_tensor(self, test_dir):
        """Save transposed (non-contiguous) tensor, verify roundtrip correctness."""
        t = torch.randn(8, 4).t()  # .t() makes it non-contiguous
        assert not t.is_contiguous()

        state = {"t": t}
        path = os.path.join(test_dir, "noncontig.hxz")
        save(state, path)

        restored = load(path)
        assert torch.allclose(t, restored["t"])

    def test_scalar_tensor(self, test_dir):
        """0-dim tensor, verify shape=[] roundtrip."""
        state = {"loss": torch.tensor(3.14)}
        path = os.path.join(test_dir, "scalar_tensor.hxz")
        save(state, path)

        restored = load(path)
        assert restored["loss"].shape == torch.Size([])
        assert torch.allclose(state["loss"], restored["loss"])

    def test_large_tensor(self, test_dir):
        """Tensor > 1 block (1MB at 128KB blocks), verify roundtrip."""
        # 256 * 1024 float32 = 1MB
        state = {"big": torch.randn(256 * 1024)}
        path = os.path.join(test_dir, "large_tensor.hxz")
        save(state, path, block_size=128 * 1024)

        restored = load(path)
        assert torch.allclose(state["big"], restored["big"])


# ═══════════════════════════════════════════════════════════════════════════════
#  Selective loading
# ═══════════════════════════════════════════════════════════════════════════════


class TestSelectiveLoad:
    def test_selective_load(self, test_dir):
        """Save 3 tensors, load 1 via keys=, verify only that key returned."""
        state = {
            "a": torch.randn(10),
            "b": torch.randn(20),
            "c": torch.randn(30),
        }
        path = os.path.join(test_dir, "selective.hxz")
        save(state, path)

        restored = load(path, keys=["b"])
        assert list(restored.keys()) == ["b"]
        assert torch.allclose(state["b"], restored["b"])

    def test_selective_load_invalid_key(self, test_dir):
        """Request non-existent key, expect ValidationError."""
        state = {"x": torch.randn(5)}
        path = os.path.join(test_dir, "selective_invalid.hxz")
        save(state, path)

        with pytest.raises(hexz.ValidationError, match="not found"):
            load(path, keys=["nonexistent"])


# ═══════════════════════════════════════════════════════════════════════════════
#  Manifest
# ═══════════════════════════════════════════════════════════════════════════════


class TestManifest:
    def test_manifest(self, test_dir):
        """Save, call manifest(), verify keys/offsets/dtypes/shapes."""
        state = {
            "weight": torch.randn(64, 32),
            "bias": torch.randn(32),
        }
        path = os.path.join(test_dir, "manifest_test.hxz")
        save(state, path)

        info = manifest(path)
        assert set(info.keys()) == {"weight", "bias"}

        # Sorted order: "bias" first, then "weight"
        assert info["bias"]["dtype"] == "float32"
        assert info["bias"]["shape"] == [32]
        assert info["bias"]["offset"] == 0
        assert info["bias"]["length"] == 32 * 4  # float32 = 4 bytes

        assert info["weight"]["dtype"] == "float32"
        assert info["weight"]["shape"] == [64, 32]
        # weight offset = bias length padded to block boundary (128 KiB default)
        assert info["weight"]["offset"] == 128 * 1024
        assert info["weight"]["length"] == 64 * 32 * 4


# ═══════════════════════════════════════════════════════════════════════════════
#  Dtype coverage
# ═══════════════════════════════════════════════════════════════════════════════


class TestDtypes:
    @pytest.mark.parametrize(
        "dtype",
        [
            torch.float16,
            torch.float32,
            torch.float64,
            torch.int8,
            torch.int32,
            torch.int64,
            torch.uint8,
            torch.bool,
        ],
    )
    def test_all_dtypes(self, test_dir, dtype):
        """Roundtrip each supported dtype."""
        if dtype == torch.bool:
            t = torch.tensor([True, False, True, True, False])
        elif dtype.is_floating_point:
            t = torch.randn(16).to(dtype)
        else:
            t = torch.randint(0, 127, (16,), dtype=dtype)

        state = {"t": t}
        path = os.path.join(test_dir, f"dtype_{dtype}.hxz")
        save(state, path)

        restored = load(path)
        assert restored["t"].dtype == dtype
        assert torch.equal(state["t"], restored["t"])

    def test_bfloat16_roundtrip(self, test_dir):
        """Specifically verify bfloat16 known values survive save/load."""
        known = torch.tensor([1.0, -2.5, 0.0, 3.14, -0.001], dtype=torch.bfloat16)
        state = {"bf": known}
        path = os.path.join(test_dir, "bfloat16.hxz")
        save(state, path)

        restored = load(path)
        assert restored["bf"].dtype == torch.bfloat16
        assert torch.equal(known, restored["bf"])


# ═══════════════════════════════════════════════════════════════════════════════
#  Cross-version dedup
# ═══════════════════════════════════════════════════════════════════════════════


class TestDedup:
    def test_cross_version_dedup(self, test_dir):
        """Save v1, save v2 with parent (most tensors same), verify v2 file smaller."""
        shared = torch.randn(256, 256)  # 256KB — large enough to see dedup
        state_v1 = {
            "shared": shared,
            "changing": torch.randn(256, 256),
        }
        v1_path = os.path.join(test_dir, "dedup_v1.hxz")
        save(state_v1, v1_path)

        state_v2 = {
            "shared": shared,  # identical
            "changing": torch.randn(256, 256),  # different
        }
        v2_path = os.path.join(test_dir, "dedup_v2.hxz")
        save(state_v2, v2_path, parent=v1_path)

        v1_size = os.path.getsize(v1_path)
        v2_size = os.path.getsize(v2_path)

        # v2 should be noticeably smaller since "shared" is deduped
        assert v2_size < v1_size, (
            f"v2 ({v2_size}) should be smaller than v1 ({v1_size}) due to dedup"
        )

    def test_sorted_determinism(self, test_dir):
        """Save same dict twice to different files, verify identical bytes."""
        state = {
            "z_last": torch.randn(32),
            "a_first": torch.randn(32),
            "m_middle": torch.randn(32),
        }
        p1 = os.path.join(test_dir, "determ_1.hxz")
        p2 = os.path.join(test_dir, "determ_2.hxz")
        save(state, p1)
        save(state, p2)

        with open(p1, "rb") as f1, open(p2, "rb") as f2:
            assert f1.read() == f2.read()


# ═══════════════════════════════════════════════════════════════════════════════
#  Scalars
# ═══════════════════════════════════════════════════════════════════════════════


class TestScalars:
    def test_scalars_roundtrip(self, test_dir):
        """Save dict with int, float, bool, str values alongside tensors."""
        state = {
            "weight": torch.randn(8, 8),
            "epoch": 42,
            "lr": 0.001,
            "is_best": True,
            "name": "my_model",
        }
        path = os.path.join(test_dir, "scalars.hxz")
        save(state, path)

        restored = load(path)
        assert torch.allclose(state["weight"], restored["weight"])
        assert restored["epoch"] == 42
        assert isinstance(restored["epoch"], int)
        assert restored["lr"] == 0.001
        assert isinstance(restored["lr"], float)
        assert restored["is_best"] is True
        assert isinstance(restored["is_best"], bool)
        assert restored["name"] == "my_model"
        assert isinstance(restored["name"], str)

    def test_selective_load_scalar(self, test_dir):
        """Selectively load only a scalar key."""
        state = {
            "weight": torch.randn(8, 8),
            "epoch": 42,
        }
        path = os.path.join(test_dir, "selective_scalar.hxz")
        save(state, path)

        restored = load(path, keys=["epoch"])
        assert list(restored.keys()) == ["epoch"]
        assert restored["epoch"] == 42


# ═══════════════════════════════════════════════════════════════════════════════
#  Error handling
# ═══════════════════════════════════════════════════════════════════════════════


class TestErrors:
    def test_not_a_checkpoint(self, test_dir):
        """Regular hexz file -> load() -> FormatError."""
        path = os.path.join(test_dir, "not_a_ckpt.hxz")
        with hexz.Writer(path) as w:
            w.add_bytes(b"just some bytes")
            w.add_metadata({"regular": "snapshot"})

        with pytest.raises(hexz.FormatError, match="Not a Hexz checkpoint"):
            load(path)

    def test_not_a_checkpoint_manifest(self, test_dir):
        """Regular hexz file -> manifest() -> FormatError."""
        path = os.path.join(test_dir, "not_a_ckpt_manifest.hxz")
        with hexz.Writer(path) as w:
            w.add_bytes(b"data")
            w.add_metadata({"other": "meta"})

        with pytest.raises(hexz.FormatError, match="Not a Hexz checkpoint"):
            manifest(path)

    def test_unsupported_value_type(self, test_dir):
        """state_dict with unsupported value type -> ValidationError."""
        state = {"bad": [1, 2, 3]}  # list is not supported
        path = os.path.join(test_dir, "bad_type.hxz")

        with pytest.raises(hexz.ValidationError, match="unsupported type"):
            save(state, path)


# ═══════════════════════════════════════════════════════════════════════════════
#  XOR delta compression
# ═══════════════════════════════════════════════════════════════════════════════


class TestXorDelta:
    def test_xor_delta_roundtrip(self, test_dir):
        """Save base, save fine-tuned with parent -> load -> torch.allclose."""
        base_state = {
            "weight": torch.randn(128, 128),
            "bias": torch.randn(128),
        }
        v1 = os.path.join(test_dir, "xor_v1.hxz")
        save(base_state, v1)

        # Fine-tune: small perturbation
        ft_state = {
            "weight": base_state["weight"] + torch.randn(128, 128) * 1e-4,
            "bias": base_state["bias"] + torch.randn(128) * 1e-4,
        }
        v2 = os.path.join(test_dir, "xor_v2.hxz")
        save(ft_state, v2, parent=v1)

        restored = load(v2)
        assert set(restored.keys()) == {"weight", "bias"}
        assert torch.allclose(ft_state["weight"], restored["weight"])
        assert torch.allclose(ft_state["bias"], restored["bias"])

    def test_xor_delta_file_smaller(self, test_dir):
        """v2 with tiny perturbation should be much smaller than v1."""
        big = torch.randn(256, 256)  # ~256 KB
        v1_state = {"big": big}
        v1 = os.path.join(test_dir, "xor_size_v1.hxz")
        save(v1_state, v1)

        # Tiny perturbation: XOR delta should compress very well
        v2_state = {"big": big + torch.randn(256, 256) * 1e-6}
        v2 = os.path.join(test_dir, "xor_size_v2.hxz")
        save(v2_state, v2, parent=v1)

        v1_size = os.path.getsize(v1)
        v2_size = os.path.getsize(v2)
        assert v2_size < v1_size * 0.5, (
            f"v2 ({v2_size}) should be < 50% of v1 ({v1_size}) with tiny perturbation"
        )

    def test_xor_delta_new_tensor_raw(self, test_dir):
        """Tensor not in parent should be stored as raw."""
        v1_state = {"old": torch.randn(64)}
        v1 = os.path.join(test_dir, "xor_new_v1.hxz")
        save(v1_state, v1)

        v2_state = {"old": torch.randn(64), "new_tensor": torch.randn(64)}
        v2 = os.path.join(test_dir, "xor_new_v2.hxz")
        save(v2_state, v2, parent=v1)

        info = manifest(v2)
        assert info["new_tensor"]["storage"] == "raw"

        # Verify roundtrip
        restored = load(v2)
        assert torch.allclose(v2_state["new_tensor"], restored["new_tensor"])

    def test_xor_delta_resized_tensor_raw(self, test_dir):
        """Tensor with different shape/size falls back to raw."""
        v1_state = {"w": torch.randn(64, 64)}
        v1 = os.path.join(test_dir, "xor_resize_v1.hxz")
        save(v1_state, v1)

        v2_state = {"w": torch.randn(128, 64)}  # different shape
        v2 = os.path.join(test_dir, "xor_resize_v2.hxz")
        save(v2_state, v2, parent=v1)

        info = manifest(v2)
        assert info["w"]["storage"] == "raw"

        restored = load(v2)
        assert torch.allclose(v2_state["w"], restored["w"])

    def test_xor_delta_manifest_fields(self, test_dir):
        """Manifest should have storage, base_offset, base_length for xor_delta."""
        v1_state = {"a": torch.randn(128)}
        v1 = os.path.join(test_dir, "xor_manifest_v1.hxz")
        save(v1_state, v1)

        v2_state = {"a": torch.randn(128)}
        v2 = os.path.join(test_dir, "xor_manifest_v2.hxz")
        save(v2_state, v2, parent=v1)

        info = manifest(v2)
        a_info = info["a"]
        assert a_info["storage"] == "xor_delta"
        assert "base_offset" in a_info
        assert "base_length" in a_info
        assert a_info["base_length"] == a_info["length"]

    def test_xor_delta_parallel_load(self, test_dir):
        """XOR delta works with num_workers=2."""
        v1_state = {
            "a": torch.randn(128, 128),
            "b": torch.randn(128, 128),
        }
        v1 = os.path.join(test_dir, "xor_parallel_v1.hxz")
        save(v1_state, v1)

        v2_state = {
            "a": v1_state["a"] + torch.randn(128, 128) * 1e-4,
            "b": v1_state["b"] + torch.randn(128, 128) * 1e-4,
        }
        v2 = os.path.join(test_dir, "xor_parallel_v2.hxz")
        save(v2_state, v2, parent=v1)

        restored = load(v2, num_workers=2)
        assert torch.allclose(v2_state["a"], restored["a"])
        assert torch.allclose(v2_state["b"], restored["b"])

    def test_xor_delta_chained(self, test_dir):
        """Chain v1 -> v2 -> v3: each parent is itself a delta checkpoint."""
        v1_state = {"w": torch.randn(128, 128)}
        v1 = os.path.join(test_dir, "chain_v1.hxz")
        save(v1_state, v1)

        v2_state = {"w": v1_state["w"] + torch.randn(128, 128) * 1e-4}
        v2 = os.path.join(test_dir, "chain_v2.hxz")
        save(v2_state, v2, parent=v1)

        # v3's parent is v2, which stores xor_delta — tests the
        # add_xor_delta_from_buffers path (parent reconstruction).
        v3_state = {"w": v2_state["w"] + torch.randn(128, 128) * 1e-4}
        v3 = os.path.join(test_dir, "chain_v3.hxz")
        save(v3_state, v3, parent=v2)

        restored = load(v3)
        assert torch.allclose(v3_state["w"], restored["w"])

        # v3 should be small (single-step delta compresses well)
        v1_size = os.path.getsize(v1)
        v3_size = os.path.getsize(v3)
        assert v3_size < v1_size

    def test_xor_delta_chained_parallel(self, test_dir):
        """Chained XOR delta works with parallel loading (num_workers=2)."""
        v1_state = {
            "a": torch.randn(128, 128),
            "b": torch.randn(128, 128),
        }
        v1 = os.path.join(test_dir, "chain_par_v1.hxz")
        save(v1_state, v1)

        v2_state = {
            "a": v1_state["a"] + torch.randn(128, 128) * 1e-4,
            "b": v1_state["b"] + torch.randn(128, 128) * 1e-4,
        }
        v2 = os.path.join(test_dir, "chain_par_v2.hxz")
        save(v2_state, v2, parent=v1)

        v3_state = {
            "a": v2_state["a"] + torch.randn(128, 128) * 1e-4,
            "b": v2_state["b"] + torch.randn(128, 128) * 1e-4,
        }
        v3 = os.path.join(test_dir, "chain_par_v3.hxz")
        save(v3_state, v3, parent=v2)

        restored = load(v3, num_workers=2)
        assert torch.allclose(v3_state["a"], restored["a"])
        assert torch.allclose(v3_state["b"], restored["b"])

    def test_save_with_base(self, test_dir):
        """Passing base= avoids recursive parent loading."""
        v1_state = {"w": torch.randn(128, 128)}
        v1 = os.path.join(test_dir, "base_v1.hxz")
        save(v1_state, v1)

        v2_state = {"w": v1_state["w"] + torch.randn(128, 128) * 1e-4}
        v2 = os.path.join(test_dir, "base_v2.hxz")
        save(v2_state, v2, parent=v1)

        # Save v3 with base= (provides v2's tensors directly)
        v3_state = {"w": v2_state["w"] + torch.randn(128, 128) * 1e-4}
        v3 = os.path.join(test_dir, "base_v3.hxz")
        save(v3_state, v3, parent=v2, base=v2_state)

        restored = load(v3)
        assert torch.allclose(v3_state["w"], restored["w"])
