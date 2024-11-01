//! Multiple-producers, single-consumer

use chute::LendingIterator;
use arrayvec::ArrayVec;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;

mod common;
use common::*;

pub fn chute_spmc_mutex(writer_threads: usize){
    let mut queue: Arc<spin::Mutex<chute::spmc::Queue<_>>> = Default::default();
    let mut reader = queue.lock().reader();
    
    let mut joins: ArrayVec<_, 64> = Default::default();
    
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let queue = queue.clone(); 
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                queue.lock().push(message::new(i));
            }
        }));
    }

    joins.push(std::thread::spawn(move || {
        for _ in 0..COUNT {
            loop{
                if let None = reader.next(){
                    yield_fn();
                } else {
                    break;
                }
            }
        }
    }));
    
    for join in joins{
        join.join().unwrap();
    }
}

pub fn chute_mpmc(writer_threads: usize){
    let queue = chute::mpmc::Queue::new();
    let mut reader = queue.reader();
    
    let mut joins: ArrayVec<_, 64> = Default::default();
    
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let mut writer = queue.writer();
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                writer.push(message::new(i));
            }
        }));
    }

    joins.push(std::thread::spawn(move || {
        for _ in 0..COUNT {
            loop{
                if let None = reader.next(){
                    yield_fn();
                } else {
                    break;
                }
            }
        }
    }));
    
    for join in joins{
        join.join().unwrap();
    }
}   

pub fn crossbeam_unbounded(writer_threads: usize){
    let (tx, rx) = crossbeam::channel::unbounded();
    
    let mut joins: ArrayVec<_, 64> = Default::default();
    
    let writer_messages = COUNT/writer_threads;
    for _ in 0..writer_threads {
        let tx = tx.clone();
        joins.push(std::thread::spawn(move || {
            for i in 0..writer_messages {
                tx.send(message::new(i)).unwrap();
            }
        }));
    }
    
    joins.push(std::thread::spawn(move || {
        for _ in 0..COUNT {
            rx.recv().unwrap();
        }
    }));        
    
    for join in joins{
        join.join().unwrap();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    use criterion::BenchmarkId;
    
    let mut group = c.benchmark_group("mpsc");
    for wt in [1, 2, 4, 8] {
        let parameter_string = format!("w:{wt} r:1");
        
        group.bench_with_input(BenchmarkId::new("chute::spmc/mutex", parameter_string.clone()), &wt
           , |b, wt| b.iter(|| chute_spmc_mutex(*wt))
        );
        
        group.bench_with_input(BenchmarkId::new("chute::mpmc", parameter_string.clone()), &wt
           , |b, wt| b.iter(|| chute_mpmc(*wt))
        );
        
        group.bench_with_input(BenchmarkId::new("crossbeam::unbounded", parameter_string.clone()), &wt
           , |b, wt| b.iter(|| crossbeam_unbounded(*wt))
        );
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);