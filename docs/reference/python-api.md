# Python API Reference

Complete reference for the Hexz Python package.

## Installation

```bash
pip install hexz
```

Or build from source:
```bash
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz
make develop
```

## Opening Snapshots

The primary way to open snapshots is using `hexz.open()`:

::: hexz.open
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

---

## Reading Snapshots

The `Reader` class is returned by `hexz.open(path, mode='r')` and provides methods for reading data:

::: hexz.Reader
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

::: hexz.AsyncReader
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

---

## Writing Snapshots

The `Writer` class is returned by `hexz.open(path, mode='w')`:

::: hexz.Writer
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3
      show_if_no_docstring: false

---

## Building Snapshots

::: hexz.build
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

::: hexz.PROFILES
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

---

## Inspection & Verification

::: hexz.inspect
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

::: hexz.verify
    options:
      show_root_heading: true
      show_source: false
      heading_level: 3

