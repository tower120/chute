//! Multicast single-producer, multiple-consumers

use chute::LendingReader;
use arrayvec::ArrayVec;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;

mod common;
use common::*;

fn chute_spmc_read(reader_threads: usize){
    let mut queue: chute::unicast::spmc::Queue<_> = Default::default();

    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    let read_len = COUNT/reader_threads;
    for _ in 0..reader_threads {
        let mut reader = queue.reader();
        joins.push(std::thread::spawn(move || {
            for _ in 0..read_len {
                let _ = loop {
                    if let Some(msg) = reader.next() {
                        break msg;
                    }
                    yield_fn();
                };
            }
        }));
    }
    
    // WRITE
    for i in 0..COUNT {
        queue.push(message::new(i));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}  

fn chute_spmc_session_read(reader_threads: usize){
    let mut queue: chute::unicast::spmc::Queue<_> = Default::default();

    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    let read_len = COUNT/reader_threads;
    for _ in 0..reader_threads {
        let mut reader = queue.reader();
        joins.push(std::thread::spawn(move || {
            let mut session = reader.session();
            for _ in 0..read_len {
                let _ = loop {
                    if let Some(msg) = session.next() {
                        break msg;
                    }
                    yield_fn();
                };
            }
        }));
    }
    
    // WRITE
    for i in 0..COUNT {
        queue.push(message::new(i));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}

pub fn crossbeam_unbounded(reader_threads: usize){
    let (tx, rx) = crossbeam::channel::unbounded();
    
    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    let read_len = COUNT/reader_threads;
    for _ in 0..reader_threads {
        let mut rx = rx.clone();         
        joins.push(std::thread::spawn(move || {
            for _ in 0..read_len {
                rx.recv().unwrap();
            }
        }));
    }
    
    // WRITE
    for i in 0..COUNT {
        tx.send(message::new(i));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    use criterion::BenchmarkId;
    
    let mut group = c.benchmark_group("spmc");
    for reader_threads in [1, 2, 4, 8] {
        let parameter_string = format!("w:1 r:{}", reader_threads);
        
        // chute::spmc
        group.bench_with_input(BenchmarkId::new("chute::spmc read", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| chute_spmc_read(*rt))
        );
        group.bench_with_input(BenchmarkId::new("chute::spmc session::read", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| chute_spmc_session_read(*rt))
        );
        
        group.bench_with_input(BenchmarkId::new("crossbeam::unbounded", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| crossbeam_unbounded(*rt))
        );
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);