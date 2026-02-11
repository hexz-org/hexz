# Strata Documentation

Welcome to the Strata documentation! This documentation follows the [Diátaxis framework](https://diataxis.fr/) for clear, effective technical documentation.

## Start Here

**New to Strata?** Read [index.md](index.md) for comprehensive navigation.

## Quick Links by Role

### ML Engineers
- [Getting Started Tutorial](tutorials/getting-started.md)
- [Your First ML Pipeline](tutorials/first-ml-pipeline.md)
- [Python API Reference](reference/python-api.md)

### Systems Engineers
- [Getting Started Tutorial](tutorials/getting-started.md)
- [Booting Your First VM](tutorials/booting-your-first-vm.md)
- [CLI Reference](reference/cli-reference.md)

### Contributors
- [Contributing Guide](project-docs/CONTRIBUTING.md)
- [System Architecture](explanation/architecture.md)
- [Architectural Decision Records](adr/)

## Documentation Structure

This documentation is organized into four quadrants:

| Quadrant | Purpose | When to Use |
|----------|---------|-------------|
| **[Tutorials](tutorials/)** | Learn by doing | You're new and want hands-on lessons |
| **[How-To Guides](how-to/)** | Solve specific problems | You have a task and need a recipe |
| **[Reference](reference/)** | Look up details | You need API or command specifications |
| **[Explanation](explanation/)** | Understand concepts | You want to understand the "why" |

### Additional Sections

- **[ADRs](adr/)** — Architectural decisions and rationale
- **[Project Docs](project-docs/)** — Contributing, roadmap, benchmarks

## Recent Changes

**2026-02-11**: Complete restructuring to Diátaxis framework
- See [RESTRUCTURING_SUMMARY.md](RESTRUCTURING_SUMMARY.md) for details
- 38 new documentation files created
- Clear persona-based navigation
- Comprehensive ADRs for major decisions

## Building Documentation

### Python API Docs (Sphinx)

```bash
make docs-python
# Output: docs/_build/html/index.html
```

### Rust API Docs (rustdoc)

```bash
make docs-rust
# Output: target/doc/strata/index.html
```

## Contributing to Documentation

Documentation contributions are welcome! Please:

1. Follow the Diátaxis framework (see [Writing Guide](#writing-guide) below)
2. Place content in the correct quadrant
3. Add cross-links to related documentation
4. Update `index.md` if adding major sections

### Writing Guide

**Tutorials** — Learning-oriented:
- Use sequential numbered steps
- Explain what will be accomplished
- Include expected output for each step
- Be encouraging and patient in tone
- No prerequisites beyond basics

**How-To Guides** — Goal-oriented:
- State the goal clearly upfront
- Assume basic knowledge
- Provide practical solutions
- Be direct and efficient
- Include troubleshooting

**Reference** — Information-oriented:
- Use consistent structure (tables, lists)
- Be accurate and complete
- Use neutral, technical tone
- No explanations of "why"
- Scannable format

**Explanation** — Understanding-oriented:
- Explain concepts and design
- Discuss trade-offs and alternatives
- Use diagrams for complex topics
- Provide historical context
- Link to related decisions (ADRs)

## Questions?

- Open an [issue](https://github.com/Alethic-Systems/strata/issues)
- Start a [discussion](https://github.com/Alethic-Systems/strata/discussions)
- See [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)

## License

Documentation is licensed under Apache 2.0, same as the Strata project.
