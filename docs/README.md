# Hexz Documentation

Welcome to the Hexz documentation. This documentation follows the [Diátaxis framework](https://diataxis.fr/).

## Start Here

**New to Hexz?** Read [index.md](index.md) for comprehensive navigation.

## Quick Links

### ML Engineers
- [Getting Started Tutorial](tutorials/getting-started.md)
- [Store Fine-tuned Models](how-to/ml-workflows/store-finetuned-models.md)
- [Python API Reference](reference/python-api.md)
- [CLI Reference](reference/cli-reference.md)

### Contributors
- [Contributing Guide](project-docs/CONTRIBUTING.md)
- [System Architecture](explanation/architecture.md)
- [Architectural Decision Records](adr/)
- [Roadmap](project-docs/ROADMAP.md)

## Documentation Structure

| Quadrant | Purpose | When to Use |
|---|---|---|
| **[Tutorials](tutorials/)** | Learn by doing | You're new and want hands-on lessons |
| **[How-To Guides](how-to/)** | Solve specific problems | You have a task and need a recipe |
| **[Reference](reference/)** | Look up details | You need API or command specifications |
| **[Explanation](explanation/)** | Understand concepts | You want to understand the "why" |

Additional:
- **[ADRs](adr/)** — Architectural decisions and rationale
- **[Project Docs](project-docs/)** — Contributing, roadmap, benchmarks

## Building Documentation

```bash
make docs-python   # Sphinx → docs/_build/html/index.html
make docs-rust     # rustdoc → target/doc/hexz/index.html
```

## Contributing to Documentation

1. Follow the Diátaxis framework (see [index.md](index.md) for the writing guide)
2. Place content in the correct quadrant
3. Add cross-links to related documentation
4. Update `index.md` if adding major sections
5. Mark any unvalidated numbers with `[UNTESTED]`

## License

Licensed under either of [Apache License 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.
