//! Multicast single-producer, multiple-consumers

use chute::LendingReader;
use arrayvec::ArrayVec;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;

mod common;
use common::*;

fn chute_spmc(reader_threads: usize){
    let mut queue: chute::spmc::Queue<_> = Default::default();

    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    for _ in 0..reader_threads {
        let mut reader = queue.reader();
        joins.push(std::thread::spawn(move || {
            for _ in 0..COUNT {
                let msg = loop {
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

fn chute_mpmc(reader_threads: usize){
    let queue = chute::mpmc::Queue::new();

    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    for _ in 0..reader_threads {
        let mut reader = queue.reader();
        joins.push(std::thread::spawn(move || {
            for _ in 0..COUNT {
                let msg = loop {
                    if let Some(msg) = reader.next() {
                        break msg;
                    }
                    yield_fn();
                };
            }
        }));
    }
    
    // WRITE
    let mut writer = queue.writer();
    for i in 0..COUNT {
        writer.push(message::new(i));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}  

fn tokio_broadcast(reader_threads: usize){
    use tokio::sync::broadcast;
    let (tx, _) = broadcast::channel(COUNT);
    
    let mut joins: ArrayVec<_, 64> = Default::default();    

    // READ
    for _ in 0..reader_threads {
        let mut reader = tx.subscribe();
        joins.push(std::thread::spawn(move || {
            for _ in 0..COUNT {
                reader.blocking_recv().unwrap();
            }
        }));
    }
    
    // WRITE
    let mut writer = tx;
    for i in 0..COUNT {
        writer.send(message::new(i));
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
        
        group.bench_with_input(BenchmarkId::new("chute::spmc", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| chute_spmc(*rt))
        );            
        
        group.bench_with_input(BenchmarkId::new("chute::mpmc", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| chute_mpmc(*rt))
        );
        
        group.bench_with_input(BenchmarkId::new("tokio::broadcast", parameter_string.clone()), &reader_threads
           , |b, rt| b.iter(|| tokio_broadcast(*rt))
        );   
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);