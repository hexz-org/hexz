# Python API Reference

Complete reference for the Strata Python package.

## Installation

```bash
pip install strata
```

Or build from source:
```bash
git clone https://github.com/Alethic-Systems/strata.git
cd strata
make develop
```

## Opening Snapshots

The primary way to open snapshots is using `strata.open()`:

::: strata.open
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

---

## Reading Snapshots

The `Reader` class is returned by `strata.open(path, mode='r')` and provides methods for reading data:

::: strata.Reader
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false
      members:
        - read
        - seek
        - tell
        - size
        - metadata
        - analyze

---

## Async Reading

For async/await support, use `AsyncReader`:

::: strata.AsyncReader
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

---

## Writing Snapshots

The `Writer` class is returned by `strata.open(path, mode='w')`:

::: strata.Writer
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

---

## ML Integration

For PyTorch and TensorFlow DataLoader integration:

::: strata.Dataset
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

::: strata.TFDataset
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

---

## Building Snapshots

::: strata.build
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

::: strata.PROFILES
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

---

## Inspection & Verification

::: strata.inspect
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

::: strata.verify
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

