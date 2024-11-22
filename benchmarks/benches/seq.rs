use chute::LendingReader;
use criterion::{criterion_group, criterion_main, Criterion};

mod common;
use common::*;

pub fn chute_spmc() {
    let mut queue = chute::spmc::Queue::new();
    let mut reader = queue.reader();
    
    for i in 0..COUNT {
        queue.push(message::new(i));
    }

    for _ in 0..COUNT {
        reader.next().unwrap();
    }
}

pub fn chute_mpmc() {
    let queue = chute::mpmc::Queue::new();
    let mut writer = queue.writer();
    let mut reader = queue.reader();
    
    for i in 0..COUNT {
        writer.push(message::new(i));
    }

    for _ in 0..COUNT {
        reader.next().unwrap();
    }
}

pub fn chute_unicast_spmc() {
    let mut queue = chute::unicast::spmc::Queue::default();
    let mut reader = queue.reader();
    
    for i in 0..COUNT {
        queue.push(message::new(i));
    }

    for _ in 0..COUNT {
        reader.next().unwrap();
    }
}

pub fn chute_unicast_spmc_session() {
    let mut queue = chute::unicast::spmc::Queue::default();
    let mut reader = queue.reader();
    
    for i in 0..COUNT {
        queue.push(message::new(i));
    }

    let mut session = reader.session();
    for _ in 0..COUNT {
        session.next().unwrap();
    }
}

pub fn crossbeam_unbounded() {
    let (tx, rx) = crossbeam::channel::unbounded();
    
    for i in 0..COUNT {
        tx.send(message::new(i)).unwrap();
    }

    for _ in 0..COUNT {
        rx.recv().unwrap();
    }        
}

pub fn flume_unbounded() {
    let (tx, rx) = flume::unbounded();
    
    for i in 0..COUNT {
        tx.send(message::new(i)).unwrap();
    }

    for _ in 0..COUNT {
        rx.recv().unwrap();
    }        
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("seq");
    // chute::broadcast
    group.bench_function("chute::spmc", |b| b.iter(|| chute_spmc()));
    group.bench_function("chute::mpmc", |b| b.iter(|| chute_mpmc()));
    // chute::unicast
    group.bench_function("chute::unicast::spmc", |b| b.iter(|| chute_unicast_spmc()));
    group.bench_function("chute::unicast::spmc session", |b| b.iter(|| chute_unicast_spmc_session()));
    
    group.bench_function("crossbeam::unbounded", |b| b.iter(|| crossbeam_unbounded()));
    group.bench_function("flume::unbounded", |b| b.iter(|| flume_unbounded()));
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);