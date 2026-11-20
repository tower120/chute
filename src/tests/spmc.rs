use std::sync::Arc;
use rand::{Rng, SeedableRng};
use crate::block::BLOCK_SIZE;
use crate::spmc::Queue;
use crate::LendingReader;
use super::StringWrapper;

fn test_spmc_mt<Value>(rt: usize, len: usize)
where
    Value: From<usize> + Into<usize> + Clone + Send + Sync + 'static,
{
    let queue: Arc<spin::Mutex<Queue<Value>>> = Default::default();

    let mut joins = Vec::new();

    // Readers
    let control_sum = (0..len).sum();
    for _ in 0..rt {
        let mut reader = queue.lock().reader();
        joins.push(std::thread::spawn(move || {
            let mut sum = 0;
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

    joins.push(std::thread::spawn(move || {
        for i in 0..len{
            queue.lock().push(i.into());
        }
    }));

    for join in joins{
        join.join().unwrap();
    }
}

#[test]
fn fuzzy_spmc(){
    const MAX_THREADS: usize = if cfg!(miri) {4 } else {16  };
    const RANGE      : usize = if cfg!(miri) {8 } else {40  } * BLOCK_SIZE;
    const REPEATS    : usize = if cfg!(miri) {10} else {1000};

    let mut rng = rand::rngs::StdRng::seed_from_u64(0xe15bb9db3dee3a0f);
    for _ in 0..REPEATS {
        let rt  = rng.gen_range(1..=MAX_THREADS);
        let len = rng.gen_range(0..RANGE);
        test_spmc_mt::<usize>(rt, len);
        test_spmc_mt::<StringWrapper>(rt, len);
    }
}