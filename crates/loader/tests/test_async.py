import pytest
import strata


@pytest.mark.asyncio
async def test_async_read(base_snap_path, raw_data_path):
    reader = await strata.AsyncStrataReader.create(base_snap_path)

    with open(raw_data_path, "rb") as f:
        raw_data = f.read()

    data = await reader.read_at(0, 10)
    assert bytes(data) == raw_data[:10]

    await reader.seek(100)
    assert reader.tell() == 100

    data2 = await reader.read(5)
    assert bytes(data2) == raw_data[100:105]


@pytest.mark.asyncio
async def test_async_context_manager(base_snap_path):
    r_obj = await strata.AsyncStrataReader.create(base_snap_path)
    async with r_obj as r:
        d = await r.read(4)
        assert len(d) == 4
