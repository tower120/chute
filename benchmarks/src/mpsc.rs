use std::path::Path;
use str_macro::str;
use std::string::String;
use crate::{read_group, EstimatesMPMC};
use crate::mpmc;

pub fn mpsc(dir_name: impl AsRef<Path>) {
    let wts = [1,2,4,8];
    let rts = [1];
    let read = |dir: &str| -> EstimatesMPMC {
        read_group(
            &std::path::Path::new(dir_name.as_ref()).join(dir)
            ,&wts, &rts
        )
    };
    
    let all: Vec<(String, EstimatesMPMC)> = vec![
        (str!("chute::spmc\nw/ mutex"), read("chute__spmc_mutex")),
        (str!("chute::mpmc"), read("chute__mpmc")),
        (str!("crossbeam::\nunbounded"), read("crossbeam__unbounded")),
        (str!("flume::\nunbounded"), read("flume__unbounded")),
    ];
    
    mpmc::chart(&all, 1, str!("mpsc"), "out/mpsc");
}