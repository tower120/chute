use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use chute::{mpmc, spmc, LendingIterator};
use std::sync::Arc;
use arrayvec::ArrayVec;

mod message {
    use std::fmt;

    const LEN: usize = 4;

    #[derive(Clone, Copy)]
    pub(crate) struct Message(#[allow(dead_code)] [usize; LEN]);
    
    impl fmt::Debug for Message {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.pad("Message")
        }
    }
    
    #[inline]
    pub(crate) fn new(num: usize) -> Message {
        Message([num; LEN])
    }    
}

const COUNT: usize = 200_000;
const WRITER_THREADS: usize = 8;

#[inline]
fn yield_fn() {
    std::thread::yield_now();
}

mod seq{
    use super::*;
    
    pub fn spmc_seq() {
        let mut queue = spmc::Queue::new();
        let mut reader = queue.lending_reader();
        
        for i in 0..COUNT {
            queue.push(message::new(i));
        }
    
        for _ in 0..COUNT {
            reader.next().unwrap();
        }
    }
    
    pub fn mpmc_seq() {
        let queue = mpmc::Queue::new();
        let mut writer = queue.writer();
        let mut reader = queue.lending_reader();
        
        for i in 0..COUNT {
            writer.push(message::new(i));
        }
    
        for _ in 0..COUNT {
            reader.next().unwrap();
        }
    }
    
    pub fn crossbeam_seq() {
        let (tx, rx) = crossbeam::channel::unbounded();
        
        for i in 0..COUNT {
            tx.send(message::new(i)).unwrap();
        }
    
        for _ in 0..COUNT {
            rx.recv().unwrap();
        }        
    }
}

mod spsc{
    use super::*;
    
    pub fn spmc_spsc(){
        let mut queue = spmc::Queue::new();
        let mut reader = queue.lending_reader();
        
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
    
    pub fn mpmc_spsc(){
        let queue = mpmc::Queue::new();
        let mut writer = queue.writer();
        let mut reader = queue.lending_reader();
        
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
    
    pub fn crossbeam_spsc(){
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
}

mod mpsc {
    use super::*;
    
    pub fn mutex_spmc_mpsc(){
        let mut queue: Arc<spin::Mutex<spmc::Queue<_>>> = Default::default();
        let mut reader = queue.lock().lending_reader();
        
        let mut joins: ArrayVec<_, 64> = Default::default();
        
        for _ in 0..WRITER_THREADS {
            let queue = queue.clone(); 
            joins.push(std::thread::spawn(move || {
                for i in 0..COUNT/WRITER_THREADS {
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
    
    pub fn mpmc_mpsc(){
        let queue = mpmc::Queue::new();
        let mut reader = queue.lending_reader();
        
        let mut joins: ArrayVec<_, 64> = Default::default();
        
        for _ in 0..WRITER_THREADS {
            let mut writer = queue.writer();
            joins.push(std::thread::spawn(move || {
                for i in 0..COUNT/WRITER_THREADS {
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
    
    pub fn crossbeam_unbounded_mpsc(){
        let (tx, rx) = crossbeam::channel::unbounded();
        
        let mut joins: ArrayVec<_, 64> = Default::default();
        
        for _ in 0..WRITER_THREADS {
            let tx = tx.clone();
            joins.push(std::thread::spawn(move || {
                for i in 0..COUNT/WRITER_THREADS {
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
    
    pub fn crossbeam_bounded_mpsc(){
        let (tx, rx) = crossbeam::channel::bounded(COUNT);
        
        let mut joins: ArrayVec<_, 64> = Default::default();
        
        for _ in 0..WRITER_THREADS {
            let tx = tx.clone();
            joins.push(std::thread::spawn(move || {
                for i in 0..COUNT/WRITER_THREADS {
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
    
}


fn criterion_benchmark(c: &mut Criterion) {
    // seq
    {
        use seq::*;
        c.bench_function("spmc seq", |b| b.iter(|| spmc_seq()));
        c.bench_function("mpmc seq", |b| b.iter(|| mpmc_seq()));
        c.bench_function("crossbeam seq", |b| b.iter(|| crossbeam_seq()));
    }
    
    // spsc
    {
        use spsc::*;
        c.bench_function("spmc spsc", |b| b.iter(|| spmc_spsc()));
        c.bench_function("mpmc spsc", |b| b.iter(|| mpmc_spsc()));
        c.bench_function("crossbeam spsc", |b| b.iter(|| crossbeam_spsc()));
    }
    
    // mpsc
    {
        use mpsc::*;
        c.bench_function("mutex<smpc> mpsc", |b| b.iter(|| mutex_spmc_mpsc()));
        c.bench_function("mpmc mpsc", |b| b.iter(|| mpmc_mpsc()));
        c.bench_function("crossbeam unbounded mpsc", |b| b.iter(|| crossbeam_unbounded_mpsc()));
        c.bench_function("crossbeam bounded mpsc", |b| b.iter(|| crossbeam_bounded_mpsc()));
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);