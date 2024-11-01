mod mpmc;
mod mpsc;
mod spsc;
mod seq;

use std::collections::{BTreeMap};
use std::fs;
use std::fs::File;
use std::env;
use std::io::Read;
use std::path::Path;
use json::JsonValue;

fn parse_json_file(fname: &Path) -> JsonValue {
    let file_content = {
        let mut content = String::new();
        let mut file = File::open(fname).expect(&format!("File {:?} not found.", fname));
        file.read_to_string(&mut content).expect("Error reading file.");
        content
    };
    
    json::parse(&file_content).expect("Error parsing json.")    
}

/// from estimates.json
fn read_estimate(fname: &Path) -> f64 {
    let parsed = parse_json_file(&fname.join("new/estimates.json")); 
    let (_, median) = parsed.entries().find(|(str, _)| *str == "median").unwrap();
    let (_, point_estimate) = median.entries().find(|(str, _)| *str == "point_estimate").unwrap();
    point_estimate.as_f64().unwrap()
}

type EstimatesMPMC = BTreeMap<usize, BTreeMap<usize, f64>>;

fn read_group(dir_name: &Path, writers: &[usize], readers: &[usize]) -> EstimatesMPMC {
    let mut wts = BTreeMap::new();
    for &wt in writers {
        let mut rts = BTreeMap::new();
        for &rt in readers {
            let time  = read_estimate(
                &dir_name.join(format!("w_{wt} r_{rt}"))
            );
            rts.insert(rt, time);
        }
        wts.insert(wt, rts);
    }
    wts
}

const CHART_WIDTH: u32 = 1000;

fn main(){
    #[derive(Eq, PartialEq)]
    enum Command{All, Mpmc, Mpsc, Spsc, Seq}
    
    let args: Vec<String> = env::args().collect();
    let command = match args.get(1).map(|s| s.as_str()) {
        None => Command::All,
        Some("mpmc") => Command::Mpmc,
        Some("mpsc") => Command::Mpsc,
        Some("spsc") => Command::Spsc,
        Some("seq")  => Command::Seq,
        Some(_) => panic!("Command unknown."),
    };
    
    let current_dir = std::env::current_dir().unwrap();
    let criterion_dir = current_dir.join("target/criterion");
    
    let _ = fs::create_dir(current_dir.join("out"));
    
    if command == Command::All || command == Command::Mpmc {
        mpmc::mpmc(criterion_dir.join("mpmc"));    
    }
    if command == Command::All || command == Command::Mpsc {
        mpsc::mpsc(criterion_dir.join("mpsc"));
    }
    if command == Command::All || command == Command::Spsc {
        spsc::spsc(criterion_dir.join("spsc"));
    }    
    if command == Command::All || command == Command::Seq {
        seq::seq(criterion_dir.join("seq"));
    }    
}