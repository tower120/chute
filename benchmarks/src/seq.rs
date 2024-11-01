use std::path::Path;
use str_macro::str;
use crate::{read_estimate};
use crate::spsc;

pub fn seq(dir_name: impl AsRef<Path>) {
    let dir_name = dir_name.as_ref();
    
    let chute_spmc = read_estimate(
        &std::path::Path::new(dir_name).join("chute__spmc")
    );
    
    let chute_mpmc = read_estimate(
        &std::path::Path::new(dir_name).join("chute__mpmc")
    );
    
    let crossbeam_unbounded = read_estimate(
        &std::path::Path::new(dir_name).join("crossbeam__unbounded")
    );
    
    let all: Vec<(String, f64)> = vec![
        (str!("chute::spmc"), chute_spmc),
        (str!("chute::mpmc"), chute_mpmc),
        (str!("crossbeam\n(unbounded)"), crossbeam_unbounded),
    ];
    
    spsc::chart(&all, str!("seq"), "out/seq");    
}