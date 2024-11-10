use std::collections::BTreeMap;
use std::path::Path;
use str_macro::str;
use charming::element::LabelPosition;
use crate::{read_group};
use crate::multi_chart::{multi_chart, MultiChartData, Visual};

pub fn mpsc(dir_name: impl AsRef<Path>) {
    let wts = [1,2,4,8];
    let read = |dir: &str| -> BTreeMap<usize, f64> {
        let data = read_group(
            &std::path::Path::new(dir_name.as_ref()).join(dir)
            ,&wts, &[1]
        );
        data.iter().map(|(&wt, readers)| (wt, readers[&1])).collect()
    };
    
    let all: MultiChartData = vec![
        (str!("chute::spmc\nw/ mutex"), read("chute__spmc_mutex")),
        (str!("chute::mpmc"), read("chute__mpmc")),
        (str!("crossbeam::\nunbounded"), read("crossbeam__unbounded")),
        (str!("flume::\nunbounded"), read("flume__unbounded")),
    ];
    
    let visual = Visual{
        title: format!("mpsc"),
        sub_chart_name: str!("writers"),
        label_pos: LabelPosition::InsideRight,
    };
    multi_chart(&all, "out/mpsc", visual);  
}