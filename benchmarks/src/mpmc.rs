use std::collections::BTreeMap;
use std::path::Path;
use charming::element::LabelPosition;
use str_macro::str;
use crate::{read_group};
use crate::multi_chart::{multi_chart, MultiChartData, Visual};

pub fn mpmc(dir_name: impl AsRef<Path>) {
    let rt  = 4; 
    let wts = [1,2,4,8];
    let read = |dir: &str| -> BTreeMap<usize, f64> {
        let data = read_group(
            &std::path::Path::new(dir_name.as_ref()).join(dir)
            ,&wts, &[rt]
        );
        data.iter().map(|(&wt, readers)| (wt, readers[&rt])).collect()
    };

    let all: MultiChartData = vec![
        (str!("chute::spmc\nw/ mutex"), read("chute__spmc_mutex")),
        (str!("chute::mpmc"), read("chute__mpmc")),
        (str!("tokio::\nbroadcast"), read("tokio__broadcast")),
    ];
    
    let visual = Visual{
        title: format!("broadcast mpmc ({rt} readers)"),
        sub_chart_name: str!("writers"),
        label_pos: LabelPosition::Right,
    };
    multi_chart(&all, "out/mpmc", visual);  
}