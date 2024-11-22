//! Multicast multiple-producers, multiple-consumers

use chute::LendingReader;
use arrayvec::ArrayVec;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;

mod common;
use common::*;

fn chute_spmc_mutex(reader_threads: usize, writer_threads: usize){
    let queue: Arc<spin::Mutex<chute::spmc::Queue<_>>> = Default::default();

    let mut joins: ArrayVec<_, 64> = Default::default();
    
    // READ
    for _ in 0..reader_threads {
        let mut reader = queue.lock().reader();
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
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let queue = queue.clone();
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                queue.lock().push(message::new(i));
            }
        }));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}  

fn chute_mpmc(reader_threads: usize, writer_threads: usize){
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
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let mut writer = queue.writer();
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                writer.push(message::new(i));
            }
        }));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}  

fn tokio_broadcast(reader_threads: usize, writer_threads: usize){
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
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let mut writer = tx.clone();
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                writer.send(message::new(i));
            }
        }));
    }    
    
    for join in joins{
        join.join().unwrap();
    }    
}

fn criterion_benchmark(c: &mut Criterion) {
    use criterion::BenchmarkId;
    
    let mut group = c.benchmark_group("mpmc");
    for writer_threads in [1, 2, 4, 8] {
        for reader_threads in [1, 2, 4, 8] {
            let input = (reader_threads, writer_threads);
            let parameter_string = format!("w:{} r:{}", writer_threads, reader_threads);
            
            group.bench_with_input(BenchmarkId::new("chute::spmc/mutex", parameter_string.clone()), &input
               , |b, (rt, wt)| b.iter(|| chute_spmc_mutex(*rt, *wt))
            );            
            
            group.bench_with_input(BenchmarkId::new("chute::mpmc", parameter_string.clone()), &input
               , |b, (rt, wt)| b.iter(|| chute_mpmc(*rt, *wt))
            );
            
            group.bench_with_input(BenchmarkId::new("tokio::broadcast", parameter_string.clone()), &input
               , |b, (rt, wt)| b.iter(|| tokio_broadcast(*rt, *wt))
            );   
        }
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);