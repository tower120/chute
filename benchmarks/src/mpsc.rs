use std::path::Path;
use str_macro::str;
use std::string::String;
use crate::{read_group, EstimatesMPMC};
use crate::mpmc;

pub fn mpsc(dir_name: impl AsRef<Path>) {
    let dir_name = dir_name.as_ref();
    
    let wts = [1,2,4,8];
    let rts = [1];
    
    let chute_spmc_w_mutex = read_group(
        &std::path::Path::new(dir_name).join("chute__spmc_mutex")
        ,&wts, &rts
    );
    
    let chute_mpmc = read_group(
        &std::path::Path::new(dir_name).join("chute__mpmc")
        ,&wts, &rts
    );
    
    let crossbeam_unbounded = read_group(
        &std::path::Path::new(dir_name).join("crossbeam__unbounded")
        ,&wts, &rts
    );
    
    let all: Vec<(String, EstimatesMPMC)> = vec![
        (str!("chute::spmc\nw/ mutex"), chute_spmc_w_mutex),
        (str!("chute::mpmc"), chute_mpmc),
        (str!("crossbeam::\nunbounded"), crossbeam_unbounded),
    ];
    
    mpmc::chart(&all, 1, str!("mpsc"), "out/mpsc");
}