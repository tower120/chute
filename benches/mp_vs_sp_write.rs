use std::hint::black_box;
use std::sync::Arc;
use criterion::{criterion_group, criterion_main, Criterion};
use event_queue::{mpmc, spmc};

const THREADS: usize = 4;

fn mpmc_write(n: usize) {
    let mut queue = mpmc::Queue::new();
    
    let mut joins = Vec::new();
    for _ in 0..THREADS {
        let mut writer = queue.writer();
        joins.push(std::thread::spawn(move || {
            for i in 0..n {
                writer.push(i);
            }
        }));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}

fn spmc_write(n: usize) {
    let mut queue: Arc<spin::Mutex<spmc::Queue<_>>> = Default::default();
    
    let mut joins = Vec::new();
    for _ in 0..THREADS {
        let mut queue = queue.clone();
        joins.push(std::thread::spawn(move || {
            for i in 0..n {
                queue.lock().push(i);
            }
        }));
    }
    
    for join in joins{
        join.join().unwrap();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    const COUNT: usize = 20000; 
    c.bench_function("mpmc write", |b| b.iter(|| mpmc_write(black_box(COUNT))));
    c.bench_function("spmc write", |b| b.iter(|| spmc_write(black_box(COUNT))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);