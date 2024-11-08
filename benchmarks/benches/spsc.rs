use chute::LendingReader;
use criterion::{criterion_group, criterion_main, Criterion};

mod common;
use common::*;

pub fn chute_spmc(){
    let mut queue = chute::spmc::Queue::new();
    let mut reader = queue.reader();
    
    let wt = std::thread::spawn(move || {
        for i in 0..COUNT {
            queue.push(message::new(i));
        }
    });

    let rt = std::thread::spawn(move || {
        for _ in 0..COUNT {
            loop{
                if let None = reader.next(){
                    yield_fn();
                } else {
                    break;
                }
            }
        }
    });
    
    wt.join().unwrap();
    rt.join().unwrap();
}

pub fn chute_mpmc(){
    let queue = chute::mpmc::Queue::new();
    let mut writer = queue.writer();
    let mut reader = queue.reader();
    
    let wt = std::thread::spawn(move || {
        for i in 0..COUNT {
            writer.push(message::new(i));
        }
    });

    let rt = std::thread::spawn(move || {
        for _ in 0..COUNT {
            loop{
                if let None = reader.next(){
                    yield_fn();
                } else {
                    break;
                }
            }
        }
    });
    
    wt.join().unwrap();
    rt.join().unwrap();
}

pub fn crossbeam_unbounded(){
    let (tx, rx) = crossbeam::channel::unbounded();
    
    let wt = std::thread::spawn(move || {
        for i in 0..COUNT {
            tx.send(message::new(i));
        }
    });

    let rt = std::thread::spawn(move || {
        for _ in 0..COUNT {
            rx.recv().unwrap();
        }
    });
    
    wt.join().unwrap();
    rt.join().unwrap();        
}

pub fn flume_unbounded(){
    let (tx, rx) = flume::unbounded();
    
    let wt = std::thread::spawn(move || {
        for i in 0..COUNT {
            tx.send(message::new(i));
        }
    });

    let rt = std::thread::spawn(move || {
        for _ in 0..COUNT {
            rx.recv().unwrap();
        }
    });
    
    wt.join().unwrap();
    rt.join().unwrap();        
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("spsc");
    group.bench_function("chute::spmc", |b| b.iter(|| chute_spmc()));
    group.bench_function("chute::mpmc", |b| b.iter(|| chute_mpmc()));
    group.bench_function("crossbeam::unbounded", |b| b.iter(|| crossbeam_unbounded()));
    group.bench_function("flume::unbounded", |b| b.iter(|| flume_unbounded()));
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);