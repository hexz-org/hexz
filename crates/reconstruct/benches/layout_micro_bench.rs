use criterion::{Criterion, criterion_group, criterion_main};

// Benchmarks will be added as research progresses.
// See IS_Proposal.pdf for planned measurements.

fn placeholder(_c: &mut Criterion) {}

criterion_group!(benches, placeholder);
criterion_main!(benches);
