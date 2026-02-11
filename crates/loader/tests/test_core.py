import strata
import pickle


def test_analyze(raw_data_path):
    stats = strata.analyze(raw_data_path)
    assert hasattr(stats, "unique_bytes")
    assert hasattr(stats, "predicted_ratio")
    assert getattr(stats, "unique_bytes", None) is not None


def test_inspect(base_snap_path):
    meta = strata.inspect(base_snap_path)
    assert meta.version == 1
    assert meta.compression is not None
    assert meta.disk_size == 1024 * 1024


def test_read_random_access(base_snap_path, raw_data_path):
    reader = strata.open(base_snap_path)

    # Read raw data to compare
    with open(raw_data_path, "rb") as f:
        raw_data = f.read()

    # Read at offset
    chunk = reader.read(10, offset=100)
    assert bytes(chunk) == raw_data[100:110]


def test_file_interface(base_snap_path, raw_data_path):
    reader = strata.open(base_snap_path)
    with open(raw_data_path, "rb") as f:
        raw_data = f.read()

    reader.seek(0)
    assert reader.tell() == 0

    data = reader.read(5)
    assert len(data) == 5
    assert data == raw_data[:5]
    assert reader.tell() == 5

    reader.seek(500)
    assert reader.tell() == 500

    reader.seek(10, 1)  # relative
    assert reader.tell() == 510

    reader.seek(-5, 2)  # from end
    assert reader.tell() == len(raw_data) - 5


def test_read_buffer(base_snap_path, raw_data_path):
    """read(buffer=...) fills buffer zero-copy and returns bytes read."""
    reader = strata.open(base_snap_path)
    with open(raw_data_path, "rb") as f:
        raw_data = f.read()
    buf = bytearray(10)
    reader.seek(50)
    n = reader.read(buffer=buf)
    assert n == 10
    assert buf == raw_data[50:60]
    reader.seek(100)
    buf2 = bytearray(20)
    m = reader.read(buffer=buf2)
    assert m == 20
    assert buf2 == raw_data[100:120]


def test_read_into_buffer(base_snap_path, raw_data_path):
    """read(buffer=buf, offset=...) fills buffer and returns bytes read; matches read(size, offset=)."""
    reader = strata.open(base_snap_path)
    with open(raw_data_path, "rb") as f:
        raw_data = f.read()

    buf = bytearray(10)
    n = reader.read(buffer=buf, offset=100)
    assert n == 10
    assert bytes(buf) == raw_data[100:110]

    buf2 = bytearray(4096)
    n2 = reader.read(buffer=buf2, offset=0)
    assert n2 == 4096
    assert bytes(buf2) == raw_data[:4096]

    buf3 = bytearray(8)
    n3 = reader.read(buffer=buf3, offset=reader.size + 1000)
    assert n3 == 0

    buf4 = bytearray(100)
    n4 = reader.read(buffer=buf4, offset=reader.size - 20)
    assert n4 == 20
    assert bytes(buf4[:20]) == raw_data[-20:]


def test_pickle(base_snap_path):
    reader = strata.open(base_snap_path)
    reader.seek(1234)

    dumped = pickle.dumps(reader)
    reader2 = pickle.loads(dumped)

    assert reader2.tell() == 1234

    # verify it works
    d = reader2.read(5)
    assert len(d) == 5
