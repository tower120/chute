use std::path::Path;
use str_macro::str;
use crate::{read_estimate};
use crate::spsc;

pub fn seq(dir_name: impl AsRef<Path>) {
    let read = |dir: &str| -> f64 {
        read_estimate(
            &std::path::Path::new(dir_name.as_ref()).join(dir)
        )
    };
    
    let all: Vec<(String, f64)> = vec![
        (str!("chute::spmc"), read("chute__spmc")),
        (str!("chute::mpmc"), read("chute__mpmc")),
        (str!("crossbeam::\nunbounded"), read("crossbeam__unbounded")),
        (str!("flume::\nunbounded"), read("flume__unbounded")),
    ];
    
    spsc::chart(&all, str!("seq"), "out/seq");    
}