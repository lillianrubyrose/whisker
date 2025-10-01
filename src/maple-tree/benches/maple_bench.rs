#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::{collections::BTreeMap, hint::black_box, sync::RwLock};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use maple_tree::MapleTree;
use rand::prelude::*;

const DATA_SIZE: usize = 10000;

fn bench_insert(c: &mut Criterion) {
	let mut group = c.benchmark_group("Insert");
	group.sample_size(20);

	group.bench_function("MapleTree_Sequential", |b| {
		b.iter(|| {
			let tree = MapleTree::<u64, 16>::new();
			for i in 0..black_box(DATA_SIZE as u64) {
				tree.store(i, i * 2);
			}
		})
	});

	group.bench_function("RwLock_BTreeMap_Sequential", |b| {
		b.iter(|| {
			let map = RwLock::new(BTreeMap::new());
			for i in 0..black_box(DATA_SIZE as u64) {
				map.write().unwrap().insert(i, i * 2);
			}
		})
	});

	let mut rng = StdRng::seed_from_u64(42);
	let mut keys: Vec<u64> = (0..DATA_SIZE as u64).collect();
	keys.shuffle(&mut rng);

	group.bench_with_input(BenchmarkId::new("MapleTree_Random", DATA_SIZE), &keys, |b, k| {
		b.iter(|| {
			let tree = MapleTree::<u64, 16>::new();
			for &key in k.iter() {
				tree.store(black_box(key), black_box(key * 2));
			}
		})
	});

	group.bench_with_input(BenchmarkId::new("RwLock_BTreeMap_Random", DATA_SIZE), &keys, |b, k| {
		b.iter(|| {
			let map = RwLock::new(BTreeMap::new());
			for &key in k.iter() {
				map.write().unwrap().insert(black_box(key), black_box(key * 2));
			}
		})
	});

	group.finish();
}

fn bench_lookup(c: &mut Criterion) {
	let mut group = c.benchmark_group("Lookup");
	group.sample_size(50);

	let maple_tree = MapleTree::<u64, 16>::new();
	let btree_map = RwLock::new(BTreeMap::new());
	{
		let mut writer = btree_map.write().unwrap();
		for i in 0..DATA_SIZE as u64 {
			maple_tree.store(i, i * 2);
			writer.insert(i, i * 2);
		}
	}
	let guard = crossbeam_epoch::pin();

	group.bench_function("MapleTree_Sequential", |b| {
		b.iter(|| {
			for i in 0..black_box(DATA_SIZE as u64) {
				black_box(maple_tree.load(i, &guard));
			}
		})
	});

	group.bench_function("RwLock_BTreeMap_Sequential", |b| {
		b.iter(|| {
			let reader = btree_map.read().unwrap();
			for i in 0..black_box(DATA_SIZE as u64) {
				black_box(reader.get(&i));
			}
		})
	});

	let mut rng = StdRng::seed_from_u64(42);
	let mut keys: Vec<u64> = (0..DATA_SIZE as u64).collect();
	keys.shuffle(&mut rng);

	group.bench_with_input(BenchmarkId::new("MapleTree_Random", DATA_SIZE), &keys, |b, k| {
		b.iter(|| {
			for &key in k.iter() {
				black_box(maple_tree.load(black_box(key), &guard));
			}
		})
	});

	group.bench_with_input(BenchmarkId::new("RwLock_BTreeMap_Random", DATA_SIZE), &keys, |b, k| {
		b.iter(|| {
			let reader = btree_map.read().unwrap();
			for &key in k.iter() {
				black_box(reader.get(&black_box(key)));
			}
		})
	});

	group.finish();
}

criterion_group!(benches, bench_insert, bench_lookup);
criterion_main!(benches);
