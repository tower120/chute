use std::sync::Arc;
use itertools::assert_equal;
use rand::{Rng, SeedableRng};
use super::StringWrapper;
use crate::block::BLOCK_SIZE;
use crate::LendingIterator;
use crate::mpmc::Queue;

#[test]
fn test_mpmc() {
    let queue: Arc<Queue<usize>> = Default::default();
    let mut reader = queue.reader();
    //let mut writer = queue.writer();

    const COUNT: usize = BLOCK_SIZE * 4;
    for i in 0..COUNT {
        queue.blocking_push(i);
        //writer.push(i);
    }

    let mut vec = Vec::new();
    while let Some(value) = reader.next() {
        vec.push(value.clone());
    }
    assert_equal(vec, 0..COUNT);
}

fn test_mpmc_mt<Value>(wt: usize, rt: usize, len: usize)
where
    Value: From<usize> + Into<usize> + Clone + Send + Sync + 'static,
{
    let queue: Arc<Queue<Value>> = Default::default();

    let mut joins = Vec::new();

    // Readers
    let control_sum = (0..len).sum();
    for _ in 0..rt {
        let mut reader = queue.reader();
        joins.push(std::thread::spawn(move || {
            let mut sum: usize = 0;
            let mut i = 0;
            loop {
                if let Some(value) = reader.next() {
                    sum += value.clone().into();

                    i += 1;
                    if i == len {
                        break;
                    }
                }
            }
            assert_eq!(sum, control_sum);
        }));
    }

    // Writers
    for t in 0..wt {
        let messages = len/wt;
        let mut writer = queue.writer();
        joins.push(std::thread::spawn(move || {
            for i in t*messages..(t+1)*messages {
                writer.push(i.into());
            }
        }));
    }

    for join in joins{
        join.join().unwrap();
    }
}

#[test]
fn fuzzy_mpmc(){
    const MAX_THREADS: usize = if cfg!(miri) {4 } else {16  };
    const RANGE      : usize = if cfg!(miri) {8 } else {40  } * BLOCK_SIZE;
    const REPEATS    : usize = if cfg!(miri) {10} else {1000};

    let mut rng = rand::rngs::StdRng::seed_from_u64(0xe15bb9db3dee3a0f);
    for _ in 0..REPEATS {
        let wt  = rng.gen_range(1..=MAX_THREADS);
        let rt  = rng.gen_range(1..=MAX_THREADS);
        let len = rng.gen_range(0..RANGE) / wt * wt;
        test_mpmc_mt::<usize>(wt, rt, len);
        test_mpmc_mt::<StringWrapper>(wt, rt, len);
    }
}