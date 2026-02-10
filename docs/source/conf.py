# Strata Python API documentation
# Build from repo root: sphinx-build -b html docs/source docs/_build/html
# Or: cd docs && make html (with strata installed: pip install -e crates/loader)

from pathlib import Path

# Add Python package to path when building from source (without installing)
_repo_root = Path(__file__).resolve().parents[2]
_loader_python = _repo_root / "crates" / "loader" / "python"
if _loader_python.exists():
    import sys

    sys.path.insert(0, str(_loader_python))

project = "Strata"
copyright = "2026, Strata contributors"
author = "Strata contributors"
release = "0.1.0-alpha"
version = "0.1.0-alpha"

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
    "sphinx.ext.viewcode",
    "sphinx.ext.intersphinx",
]

templates_path = ["_templates"]
exclude_patterns = []
# Omit html_static_path if _static is not used (avoids "does not exist" warning)
html_static_path = []
html_theme = "alabaster"
html_title = "Strata Python API"

autodoc_default_options = {
    "members": True,
    "member-order": "bysource",
    "undoc-members": False,
    "show-inheritance": True,
}
autodoc_typehints = "description"
napoleon_include_special_with_doc = True

intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
    "numpy": ("https://numpy.org/doc/stable/", None),
    "torch": ("https://pytorch.org/docs/stable/", None),
}
