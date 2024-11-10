use std::collections::BTreeMap;
use std::path::Path;
use charming::element::LabelPosition;
use str_macro::str;
use crate::{read_group};
use crate::multi_chart::{multi_chart, MultiChartData, Visual};

pub fn spmc(dir_name: impl AsRef<Path>) {
    let rts = [1,2,4,8];
    let read = |dir: &str| -> BTreeMap<usize, f64> {
        let data = read_group(
            &std::path::Path::new(dir_name.as_ref()).join(dir)
            ,&[1], &rts
        );
        data[&1].clone()
    };

    let all: MultiChartData = vec![
        (str!("chute::spmc"), read("chute__spmc")),
        (str!("chute::mpmc"), read("chute__mpmc")),
        (str!("tokio::\nbroadcast"), read("tokio__broadcast")),
    ];
    
    let visual = Visual{
        title: str!("broadcast spmc"),
        sub_chart_name: str!("readers"),
        label_pos: LabelPosition::Right,
    };
    multi_chart(&all, "out/spmc", visual);    
}