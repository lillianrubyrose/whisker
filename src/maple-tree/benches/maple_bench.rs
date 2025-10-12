use std::{collections::BTreeMap, hint::black_box, ops::Range};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use maple_tree::MapleTree;

const RANGE_SIZES: &[usize] = &[1000];
const NUM_RANGES: &[usize] = &[1000];

fn generate_nonoverlapping_ranges(num_ranges: usize, range_size: usize) -> Vec<Range<usize>> {
	(0..num_ranges)
		.map(|i| {
			let start = i * (range_size + 10);
			start..start + range_size
		})
		.collect()
}

fn bench_maple_tree_insert(c: &mut Criterion) {
	let mut group = c.benchmark_group("maple_tree_insert");

	for &num_ranges in NUM_RANGES {
		for &range_size in RANGE_SIZES {
			group.throughput(Throughput::Elements(num_ranges as u64));

			let ranges = generate_nonoverlapping_ranges(num_ranges, range_size);
			let values: Vec<_> = (0..num_ranges).collect();

			group.bench_with_input(
				BenchmarkId::from_parameter(format!("n={}_size={}", num_ranges, range_size)),
				&(ranges.clone(), values.clone()),
				|b, (ranges, values)| {
					b.iter(|| {
						let tree = MapleTree::new();
						for (range, &value) in ranges.iter().zip(values.iter()) {
							tree.store_range(black_box(range.start), black_box(range.end), black_box(value));
						}
					});
				},
			);
		}
	}

	group.finish();
}

fn bench_btreemap_insert(c: &mut Criterion) {
	let mut group = c.benchmark_group("btreemap_insert");

	for &num_ranges in NUM_RANGES {
		for &range_size in RANGE_SIZES {
			group.throughput(Throughput::Elements(num_ranges as u64));

			let ranges = generate_nonoverlapping_ranges(num_ranges, range_size);

			group.bench_with_input(
				BenchmarkId::from_parameter(format!("n={}_size={}", num_ranges, range_size)),
				&ranges,
				|b, ranges| {
					b.iter(|| {
						let mut map = BTreeMap::new();
						for (i, range) in ranges.iter().enumerate() {
							// BTreeMap needs an entry per index in the range
							for idx in range.start..=range.end {
								map.insert(black_box(idx), black_box(i));
							}
						}
						map
					});
				},
			);
		}
	}

	group.finish();
}

fn bench_sparse_ranges(c: &mut Criterion) {
	let mut group = c.benchmark_group("sparse_ranges");

	let ranges: Vec<Range<usize>> = (0..100usize)
		.map(|i| {
			let base = i * 0x1000_0000_0000;
			base..base + 4096
		})
		.collect();

	let tree = MapleTree::new();
	let values: Vec<_> = (0..100).collect();

	for (range, &value) in ranges.iter().zip(values.iter()) {
		tree.store_range(range.start, range.end, value);
	}

	group.bench_function("maple_tree_sparse", |b| {
		b.iter(|| {
			for range in &ranges[0..10] {
				tree.load(range.start + 100);
			}
		});
	});

	let mut map = BTreeMap::new();

	for (i, range) in ranges.iter().enumerate() {
		for idx in range.start..=range.end {
			map.insert(idx, i);
		}
	}

	group.bench_function("btreemap_sparse", |b| {
		b.iter(|| {
			for range in &ranges[0..10] {
				map.get(&(range.start + 100));
			}
		});
	});

	group.finish();
}

criterion_group!(
	benches,
	bench_maple_tree_insert,
	bench_btreemap_insert,
	bench_sparse_ranges,
);

criterion_main!(benches);
